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

        let total_size: u64 = volumes.iter().map(|(_, sz)| *sz).sum();

        let first_path = volumes[0].0.clone();
        let first_reader = BufReader::new(File::open(&first_path)?);

        Ok(MultiVolumeReader {
            base_name,
            volumes,
            current_index: 0,
            current_reader: Some(first_reader),
            global_position: 0,
            total_size,
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

        let mut remaining = pos;
        for (i, (path, fsize)) in self.volumes.iter().enumerate() {
            if remaining < *fsize {
                let mut reader = BufReader::new(File::open(path)?);
                reader.seek(SeekFrom::Start(remaining))?;
                self.current_index = i;
                self.current_reader = Some(reader);
                self.global_position = pos;
                return Ok(pos);
            }
            remaining -= *fsize;
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
        if self.max_part_size == 0 {
            // No splitting — write everything to current file
            let file = self.current_file.as_mut().unwrap();
            let n = file.write(buf)?;
            self.current_bytes += n as u64;
            return Ok(n);
        }

        let mut total_written = 0usize;
        let mut remaining = buf;

        while !remaining.is_empty() {
            let file = self.current_file.as_mut().unwrap();
            let space_left = self.max_part_size.saturating_sub(self.current_bytes);

            if space_left == 0 {
                // Current volume is full — advance to next
                self.current_index += 1;
                let dir = self.base_path.parent().unwrap_or(Path::new("."));
                let base = self.base_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("archive");
                let next_path = format_volume_path(dir, base, self.pattern, self.current_index);
                self.current_file = Some(File::create(&next_path)?);
                self.current_bytes = 0;
                self.created_paths.push(next_path);
                continue;
            }

            let chunk = if (remaining.len() as u64) <= space_left {
                remaining
            } else {
                &remaining[..space_left as usize]
            };

            let n = file.write(chunk)?;
            self.current_bytes += n as u64;
            total_written += n;
            if n < chunk.len() {
                // Short write — try again (don't advance volume)
                remaining = &remaining[n..];
                continue;
            }
            remaining = &remaining[n..];
        }

        Ok(total_written)
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
        if self.global_position >= self.total_size {
            return Ok(0);
        }

        let fsize = self.volumes[self.current_index].1;

        // Bytes remaining in current volume
        let end_of_prev: u64 = self.volumes[..self.current_index]
            .iter()
            .map(|(_, s)| s)
            .sum();
        let pos_in_volume = self.global_position - end_of_prev;
        let _remaining_in_volume = fsize.saturating_sub(pos_in_volume);

        match &mut self.current_reader {
            Some(reader) => {
                let n = reader.read(buf)?;
                self.global_position += n as u64;

                // If we exhausted this volume and there are more, advance
                if n == 0 && self.current_index + 1 < self.volumes.len() {
                    self.current_index += 1;
                    let next_path = &self.volumes[self.current_index].0;
                    self.current_reader = Some(BufReader::new(File::open(next_path)?));
                    return self.read(buf);
                }
                Ok(n)
            }
            None => {
                // Open current volume if not already open
                let path = &self.volumes[self.current_index].0;
                let mut reader = BufReader::new(File::open(path)?);
                let end_of_prev: u64 = self.volumes[..self.current_index]
                    .iter()
                    .map(|(_, s)| s)
                    .sum();
                let pos_in_volume = self.global_position - end_of_prev;
                reader.seek(SeekFrom::Start(pos_in_volume))?;
                self.current_reader = Some(reader);
                self.read(buf)
            }
        }
    }
}

impl Seek for MultiVolumeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(offset) => {
                if offset >= 0 {
                    (self.total_size as i64 + offset) as u64
                } else {
                    self.total_size.saturating_sub((-offset) as u64)
                }
            }
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.global_position + offset as u64
                } else {
                    self.global_position.saturating_sub((-offset) as u64)
                }
            }
        };
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
    let dir = first.parent().unwrap_or(Path::new("."));
    let mut volumes = Vec::new();
    let mut num = start_num;

    loop {
        let vol_path = format_volume_path(dir, base_name, pattern, num);
        if !vol_path.exists() {
            break;
        }
        let meta = std::fs::metadata(&vol_path)?;
        volumes.push((vol_path, meta.len()));
        num += 1;
    }

    Ok(volumes)
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
    fn test_single_volume_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("only.001");
        File::create(&path).unwrap().write_all(b"data").unwrap();
        assert!(MultiVolumeReader::open(&path).is_err());
    }
}
