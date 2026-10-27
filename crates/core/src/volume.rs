//! Split volume (multi-part file) reader.
//!
//! Supports .001/.002, .7z.001/.7z.002, and .i00/.i01 naming patterns.
//! Chains volumes into a single Read + Seek stream.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Pattern for volume file naming.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VolumePattern {
    /// .NNN (HJSplit, generic): archive.001, archive.002, ...
    Nnn,
    /// .7z.NNN: archive.7z.001, archive.7z.002, ...
    SevenZipNnn,
    /// .iNN (ImgBurn): archive.i00, archive.i01, ...
    INn,
}

/// Reader that chains multiple volume files into a single logical stream.
pub struct MultiVolumeReader {
    base_name: String,
    pub volumes: Vec<(PathBuf, u64)>, // (path, file_size)
    current_index: usize,
    current_reader: Option<BufReader<File>>,
    global_position: u64,
    total_size: u64,
    /// Starting offsets, followed by the total length (including empty parts).
    volume_offsets: Vec<u64>,
}

impl MultiVolumeReader {
    /// Open the first volume and discover siblings.
    pub fn open(first: &Path) -> io::Result<Self> {
        let (base_name, pattern, first_num) = parse_volume_pattern(first)?;
        let volumes = discover_volumes(first, &base_name, pattern, first_num)?;

        if volumes.len() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "expected split volume set, found only 1 part: {}",
                    first.display()
                ),
            ));
        }

        let mut volume_offsets = Vec::with_capacity(volumes.len() + 1);
        let mut total_size = 0u64;
        volume_offsets.push(total_size);
        for (_, size) in &volumes {
            total_size = total_size.checked_add(*size).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "volume set size overflow")
            })?;
            volume_offsets.push(total_size);
        }

        let first_path = volumes[0].0.clone();
        let first_reader = BufReader::new(File::open(&first_path)?);

        Ok(MultiVolumeReader {
            base_name,
            volumes,
            current_index: 0,
            current_reader: Some(first_reader),
            global_position: 0,
            total_size,
            volume_offsets,
        })
    }

    pub fn base_name(&self) -> &str {
        &self.base_name
    }

    pub fn total_len(&self) -> u64 {
        self.total_size
    }

    /// Seek to a global position, switching volumes as needed.
    fn seek_global(&mut self, pos: u64) -> io::Result<u64> {
        if pos > self.total_size {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek past end"));
        }

        if pos < self.total_size {
            let i = self.volume_offsets.partition_point(|&start| start <= pos) - 1;
            let mut reader = BufReader::new(File::open(&self.volumes[i].0)?);
            reader.seek(SeekFrom::Start(pos - self.volume_offsets[i]))?;
            self.current_index = i;
            self.current_reader = Some(reader);
            self.global_position = pos;
            return Ok(pos);
        }
        // Past last byte — position at end
        self.current_index = self.volumes.len() - 1;
        self.current_reader = None;
        self.global_position = self.total_size;
        Ok(self.total_size)
    }
}

/// Writer that splits output across multiple volume files.
///
/// Each volume is limited to `max_part_size` bytes (except possibly the
/// last).  When a volume fills up a new file is created automatically,
/// following the naming convention of `VolumePattern::Nnn` (`.001`,
/// `.002`, …).
///
/// # Example
///
/// ```no_run
/// use geezipx_core::volume::{MultiVolumeWriter, VolumePattern};
/// use std::io::Write;
///
/// let mut w = MultiVolumeWriter::new(
///     "archive.zip".as_ref(),
///     VolumePattern::Nnn,
///     10 * 1024 * 1024, // 10 MiB per volume
///     1,
/// ).unwrap();
/// w.write_all(b"data").unwrap();
/// let paths = w.finish().unwrap();
/// ```
pub struct MultiVolumeWriter {
    base_path: PathBuf,
    pattern: VolumePattern,
    max_part_size: u64,
    current_index: u32,
    current_file: Option<File>,
    current_bytes: u64,
    created_paths: Vec<PathBuf>,
}

impl MultiVolumeWriter {
    /// Create a new volume writer.
    ///
    /// * `base_path` — full path of the desired output (e.g. `"archive.zip"`).
    ///   The first volume will be written to `"archive.zip.001"` etc.
    /// * `pattern` — volume naming convention.
    /// * `max_part_size` — maximum bytes per volume.
    /// * `start_num` — starting index (usually `1` for Nnn, `0` for INn).
    pub fn new(
        base_path: &Path,
        pattern: VolumePattern,
        max_part_size: u64,
        start_num: u32,
    ) -> io::Result<Self> {
        let dir = base_path.parent().unwrap_or(Path::new("."));
        let base = base_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archive");
        let vol_path = format_volume_path(dir, base, pattern, start_num);
        let file = File::create(&vol_path)?;

        Ok(MultiVolumeWriter {
            base_path: base_path.to_path_buf(),
            pattern,
            max_part_size,
            current_index: start_num,
            current_file: Some(file),
            current_bytes: 0,
            created_paths: vec![vol_path],
        })
    }

    /// Finish writing and return the list of created volume paths.
    pub fn finish(mut self) -> io::Result<Vec<PathBuf>> {
        if let Some(ref f) = self.current_file {
            f.sync_all()?;
        }
        self.current_file = None;
        Ok(self.created_paths)
    }
}

impl Write for MultiVolumeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if self.max_part_size != 0 && self.current_bytes == self.max_part_size {
            let next_index = self.current_index.checked_add(1).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "volume index overflow")
            })?;
            let dir = self.base_path.parent().unwrap_or(Path::new("."));
            let base = self
                .base_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("archive");
            let next_path = format_volume_path(dir, base, self.pattern, next_index);
            let next_file = File::create(&next_path)?;
            // Commit the new index only after creation succeeds so retries use
            // the same filename.
            self.current_file = Some(next_file);
            self.current_index = next_index;
            self.current_bytes = 0;
            self.created_paths.push(next_path);
        }

        let chunk_len = if self.max_part_size == 0 {
            buf.len()
        } else {
            (buf.len() as u64).min(self.max_part_size - self.current_bytes) as usize
        };
        // One underlying write per call preserves short/zero writes and never
        // hides successfully written bytes behind a later part's error.
        let n = self
            .current_file
            .as_mut()
            .unwrap()
            .write(&buf[..chunk_len])?;
        self.current_bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(ref mut f) = self.current_file {
            f.flush()
        } else {
            Ok(())
        }
    }
}

impl Read for MultiVolumeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.global_position >= self.total_size {
            return Ok(0);
        }

        loop {
            let end = self.volume_offsets[self.current_index + 1];
            if self.global_position == end {
                // Open first, then advance: a failed open must remain retryable.
                let next_index = self.current_index + 1;
                let reader = BufReader::new(File::open(&self.volumes[next_index].0)?);
                self.current_reader = Some(reader);
                self.current_index = next_index;
                continue;
            }

            if self.current_reader.is_none() {
                let mut reader = BufReader::new(File::open(&self.volumes[self.current_index].0)?);
                reader.seek(SeekFrom::Start(
                    self.global_position - self.volume_offsets[self.current_index],
                ))?;
                self.current_reader = Some(reader);
            }
            let len = (buf.len() as u64).min(end - self.global_position) as usize;
            let n = self
                .current_reader
                .as_mut()
                .unwrap()
                .read(&mut buf[..len])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "volume truncated after discovery",
                ));
            }
            self.global_position += n as u64;
            return Ok(n);
        }
    }
}

impl Seek for MultiVolumeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(offset) => self.total_size.checked_add_signed(offset),
            SeekFrom::Current(offset) => self.global_position.checked_add_signed(offset),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek outside volume set"))?;
        self.seek_global(target)
    }
}

/// Check if a path looks like a split volume filename.
pub fn is_volume_filename(path: &Path) -> bool {
    parse_volume_pattern(path).is_ok()
}

// ----- internal helpers -----

fn parse_volume_pattern(path: &Path) -> io::Result<(String, VolumePattern, u32)> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid filename"))?;

    // .7z.NNN pattern: archive.7z.001 (must be checked before .NNN)
    if let Some(captures) = regex_match_7znn(name) {
        return Ok(captures);
    }
    // .iNN pattern: archive.i00
    if let Some(captures) = regex_match_inn(name) {
        return Ok(captures);
    }
    // .NNN pattern: archive.001
    if let Some(captures) = regex_match_nnn(name) {
        return Ok(captures);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "not a volume filename",
    ))
}

fn regex_match_nnn(name: &str) -> Option<(String, VolumePattern, u32)> {
    // Match base.NNN where NNN is exactly 3 digits
    let dot_pos = name.rfind('.')?;
    let ext = &name[dot_pos + 1..];
    if ext.len() == 3 && ext.chars().all(|c| c.is_ascii_digit()) {
        let num = ext.parse::<u32>().ok()?;
        return Some((name[..dot_pos].to_string(), VolumePattern::Nnn, num));
    }
    None
}

fn regex_match_7znn(name: &str) -> Option<(String, VolumePattern, u32)> {
    // Match base.7z.NNN
    let parts: Vec<&str> = name.rsplitn(3, '.').collect();
    if parts.len() == 3
        && parts[1] == "7z"
        && parts[0].len() == 3
        && parts[0].chars().all(|c| c.is_ascii_digit())
    {
        let num = parts[0].parse::<u32>().ok()?;
        let base = parts[2].to_string();
        return Some((base, VolumePattern::SevenZipNnn, num));
    }
    None
}

fn regex_match_inn(name: &str) -> Option<(String, VolumePattern, u32)> {
    // Match base.iNN where NN is 2 digits
    let dot_pos = name.rfind(".i")?;
    let ext = &name[dot_pos + 2..];
    if ext.len() == 2 && ext.chars().all(|c| c.is_ascii_digit()) {
        let num = ext.parse::<u32>().ok()?;
        let base = &name[..dot_pos];
        return Some((base.to_string(), VolumePattern::INn, num));
    }
    None
}

fn discover_volumes(
    first: &Path,
    base_name: &str,
    pattern: VolumePattern,
    start_num: u32,
) -> io::Result<Vec<(PathBuf, u64)>> {
    let dir = first
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut volumes = Vec::new();
    let mut num = start_num;

    loop {
        let vol_path = format_volume_path(dir, base_name, pattern, num);
        let meta = match std::fs::metadata(&vol_path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        };
        volumes.push((vol_path, meta.len()));
        num = num
            .checked_add(1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "volume index overflow"))?;
    }

    // A gap is corruption if a later part of the same set exists. Scan once
    // after the contiguous prefix, rather than silently accepting the prefix.
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if let Some(later_num) = entry
            .file_name()
            .to_str()
            .and_then(|name| volume_number(name, base_name, pattern))
        {
            if later_num > num {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "missing volume: {} (later part {} exists)",
                        format_volume_path(dir, base_name, pattern, num).display(),
                        entry.path().display()
                    ),
                ));
            }
        }
    }

    Ok(volumes)
}

/// Accept indices beyond the minimum padding width too (.1000, .i100).
fn volume_number(name: &str, base: &str, pattern: VolumePattern) -> Option<u32> {
    let suffix = match pattern {
        VolumePattern::Nnn => name.strip_prefix(base)?.strip_prefix('.')?,
        VolumePattern::SevenZipNnn => name.strip_prefix(base)?.strip_prefix(".7z.")?,
        VolumePattern::INn => name.strip_prefix(base)?.strip_prefix(".i")?,
    };
    let min_width = if pattern == VolumePattern::INn { 2 } else { 3 };
    if suffix.len() < min_width || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn format_volume_path(dir: &Path, base: &str, pattern: VolumePattern, num: u32) -> PathBuf {
    match pattern {
        VolumePattern::Nnn => dir.join(format!("{}.{:03}", base, num)),
        VolumePattern::SevenZipNnn => dir.join(format!("{}.7z.{:03}", base, num)),
        VolumePattern::INn => dir.join(format!("{}.i{:02}", base, num)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_volumes(dir: &Path, base: &str, data: &[u8], parts: usize) -> Vec<PathBuf> {
        let chunk_size = data.len().div_ceil(parts);
        let mut paths = Vec::new();
        for i in 0..parts {
            let path = dir.join(format!("{}.{:03}", base, i + 1));
            let start = i * chunk_size;
            let end = std::cmp::min(start + chunk_size, data.len());
            let mut f = File::create(&path).unwrap();
            f.write_all(&data[start..end]).unwrap();
            paths.push(path);
        }
        paths
    }

    #[test]
    fn test_parse_volume_pattern_nnn() {
        let p = Path::new("archive.001");
        let (base, pat, num) = parse_volume_pattern(p).unwrap();
        assert_eq!(base, "archive");
        assert_eq!(pat, VolumePattern::Nnn);
        assert_eq!(num, 1);
    }

    #[test]
    fn test_parse_volume_pattern_7znn() {
        let p = Path::new("archive.7z.001");
        let (base, pat, num) = parse_volume_pattern(p).unwrap();
        assert_eq!(base, "archive");
        assert_eq!(pat, VolumePattern::SevenZipNnn);
        assert_eq!(num, 1);
    }

    #[test]
    fn test_parse_volume_pattern_inn() {
        let p = Path::new("archive.i00");
        let (base, pat, num) = parse_volume_pattern(p).unwrap();
        assert_eq!(base, "archive");
        assert_eq!(pat, VolumePattern::INn);
        assert_eq!(num, 0);
    }

    #[test]
    fn test_non_volume() {
        assert!(parse_volume_pattern(Path::new("archive.zip")).is_err());
        assert!(parse_volume_pattern(Path::new("archive.tar.gz")).is_err());
    }

    #[test]
    fn test_read_across_volumes() {
        let tmp = tempfile::tempdir().unwrap();
        let data = b"Hello world! This is test data spanning multiple volumes.";
        let paths = make_volumes(tmp.path(), "test", data, 3);

        let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();
        assert_eq!(reader.total_len(), data.len() as u64);

        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn test_seek_across_volumes() {
        let tmp = tempfile::tempdir().unwrap();
        // Create exact 200-byte chunks
        let mut data = Vec::new();
        for i in 0u8..6 {
            data.extend_from_slice(&[i; 100]);
        } // 600 bytes
        let chunk_size = 200;
        let parts = 3;
        let mut paths = Vec::new();
        for i in 0..parts {
            let path = tmp.path().join(format!("test.{:03}", i + 1));
            let start = i * chunk_size;
            let end = std::cmp::min(start + chunk_size, data.len());
            let mut f = File::create(&path).unwrap();
            f.write_all(&data[start..end]).unwrap();
            paths.push(path);
        }

        let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();

        // Seek to position 350 (should be in volume 2, all-3 region)
        reader.seek(SeekFrom::Start(350)).unwrap();
        let mut buf = [0u8; 10];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [3u8; 10]);

        // Seek to end
        reader.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(reader.global_position, 600);
    }

    #[test]
    fn test_empty_read_preserves_part_and_position() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = make_volumes(tmp.path(), "test", b"abcdef", 3);
        let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();
        assert_eq!(reader.read(&mut []).unwrap(), 0);
        assert_eq!(reader.current_index, 0);
        assert_eq!(reader.global_position, 0);
        let mut buf = [0; 2];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"ab");
        assert_eq!(reader.read(&mut []).unwrap(), 0);
        assert_eq!(reader.current_index, 0);
        assert_eq!(reader.global_position, 2);
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"cd");
    }

    #[test]
    fn test_invalid_seeks_leave_position_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = make_volumes(tmp.path(), "test", b"abcdef", 3);
        let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();
        reader.seek(SeekFrom::Start(2)).unwrap();
        for seek in [
            SeekFrom::Current(-3),
            SeekFrom::Current(i64::MIN),
            SeekFrom::End(-7),
            SeekFrom::End(i64::MIN),
            SeekFrom::End(i64::MAX),
            SeekFrom::Start(u64::MAX),
        ] {
            assert_eq!(
                reader.seek(seek).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(reader.global_position, 2);
        }
        let mut buf = [0; 2];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"cd");
        assert_eq!(reader.seek(SeekFrom::End(-6)).unwrap(), 0);
    }

    #[test]
    fn test_empty_parts_and_boundary_seeks() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 1..=128 {
            std::fs::write(tmp.path().join(format!("test.{i:03}")), b"").unwrap();
        }
        std::fs::write(tmp.path().join("test.129"), b"data").unwrap();
        std::fs::write(tmp.path().join("test.130"), b"").unwrap();
        let mut reader = MultiVolumeReader::open(&tmp.path().join("test.001")).unwrap();
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"data");
        reader.seek(SeekFrom::Start(0)).unwrap();
        assert_eq!(reader.current_index, 128);
        assert_eq!(reader.seek(SeekFrom::End(0)).unwrap(), 4);
        assert_eq!(reader.read(&mut [0]).unwrap(), 0);
    }

    #[test]
    fn test_reader_retries_failed_part_open() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = make_volumes(tmp.path(), "test", b"abcdef", 3);
        let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();
        let mut buf = [0; 2];
        reader.read_exact(&mut buf).unwrap();
        std::fs::remove_file(&paths[1]).unwrap();
        assert!(reader.read(&mut buf).is_err());
        assert_eq!(reader.current_index, 0);
        assert_eq!(reader.global_position, 2);
        std::fs::write(&paths[1], b"cd").unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"cd");
    }

    #[test]
    fn test_reader_rejects_part_truncated_after_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = make_volumes(tmp.path(), "test", b"abcdef", 3);
        let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();
        File::options()
            .write(true)
            .open(&paths[0])
            .unwrap()
            .set_len(1)
            .unwrap();
        assert_eq!(
            reader.read_to_end(&mut Vec::new()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn test_missing_middle_part_is_rejected_for_each_pattern() {
        for (pattern, start) in [
            (VolumePattern::Nnn, 1),
            (VolumePattern::SevenZipNnn, 1),
            (VolumePattern::INn, 0),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            for num in [start, start + 1, start + 3] {
                std::fs::write(format_volume_path(tmp.path(), "test", pattern, num), b"x").unwrap();
            }
            let first = format_volume_path(tmp.path(), "test", pattern, start);
            let error = MultiVolumeReader::open(&first).err().unwrap();
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
            assert!(error.to_string().contains("missing volume:"));
        }
    }

    #[test]
    fn test_gap_scan_handles_large_indices_and_ignores_other_sets() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = make_volumes(tmp.path(), "test", b"abcd", 2);
        std::fs::write(tmp.path().join("other.004"), b"x").unwrap();
        std::fs::write(tmp.path().join("test.7z.004"), b"x").unwrap();
        assert!(MultiVolumeReader::open(&paths[0]).is_ok());
        std::fs::write(tmp.path().join("test.1000"), b"x").unwrap();
        assert!(MultiVolumeReader::open(&paths[0]).is_err());
    }

    #[test]
    fn test_writer_partial_progress_and_create_retry() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("test");
        let blocked = tmp.path().join("test.002");
        std::fs::create_dir(&blocked).unwrap();
        let mut writer = MultiVolumeWriter::new(&base, VolumePattern::Nnn, 3, 1).unwrap();
        assert_eq!(writer.write(b"abcdef").unwrap(), 3);
        assert_eq!(std::fs::read(tmp.path().join("test.001")).unwrap(), b"abc");
        assert!(writer.write(b"def").is_err());
        assert!(writer.write(b"def").is_err());
        assert_eq!(writer.current_index, 1);
        assert_eq!(writer.write(b"").unwrap(), 0);
        assert_eq!(writer.current_index, 1);
        std::fs::remove_dir(&blocked).unwrap();
        writer.write_all(b"defg").unwrap();
        let paths = writer.finish().unwrap();
        assert_eq!(paths.len(), 3);
        assert_eq!(std::fs::read(&paths[1]).unwrap(), b"def");
        assert_eq!(std::fs::read(&paths[2]).unwrap(), b"g");
    }

    #[test]
    fn test_writer_index_overflow_is_retryable_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut writer =
            MultiVolumeWriter::new(&tmp.path().join("test"), VolumePattern::Nnn, 1, u32::MAX)
                .unwrap();
        assert_eq!(writer.write(b"ab").unwrap(), 1);
        for _ in 0..2 {
            assert_eq!(
                writer.write(b"b").unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(writer.current_index, u32::MAX);
        }
    }

    #[test]
    fn test_writer_round_trip_and_unlimited_output() {
        for (pattern, start) in [
            (VolumePattern::Nnn, 1),
            (VolumePattern::SevenZipNnn, 1),
            (VolumePattern::INn, 0),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let mut writer =
                MultiVolumeWriter::new(&tmp.path().join("test"), pattern, 2, start).unwrap();
            writer.write_all(b"abcdefg").unwrap();
            let paths = writer.finish().unwrap();
            assert_eq!(paths.len(), 4);
            let mut reader = MultiVolumeReader::open(&paths[0]).unwrap();
            let mut out = Vec::new();
            reader.read_to_end(&mut out).unwrap();
            assert_eq!(out, b"abcdefg");
        }
        let tmp = tempfile::tempdir().unwrap();
        let mut writer =
            MultiVolumeWriter::new(&tmp.path().join("test"), VolumePattern::Nnn, 0, 1).unwrap();
        writer.write_all(b"abc").unwrap();
        writer.write_all(b"def").unwrap();
        let paths = writer.finish().unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), b"abcdef");
    }

    #[test]
    fn test_single_volume_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("only.001");
        File::create(&path).unwrap().write_all(b"data").unwrap();
        assert!(MultiVolumeReader::open(&path).is_err());
    }
}
