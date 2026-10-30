//! UDF (Universal Disk Format) filesystem reader and writer (`.udf`).
//!
//! Reading uses `hadris_udf::UdfFs` for directory listing, with manual file
//! extraction because `hadris-udf` does not expose a public file-reading API.
//!
//! Writing produces UDF 2.01 images via `hadris_udf::write::UdfWriter`.
//! Entries are buffered in memory during `add_entry_from_reader` and written
//! in a single pass when `finish()` is called. The current writer API requires
//! buffered input; UDF itself does not require whole-image buffering.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
// use std::sync::Arc;

use hadris_udf::dir::UdfDirEntry;
use hadris_udf::write::{SimpleDir, SimpleFile, UdfWriteOptions, UdfWriter as HadrisUdfWriter};
use hadris_udf::UdfFs;

use crate::archive::{is_entry_path_dangerous, ArchiveReader, ArchiveWriter, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Maximum single file size for UDF (no practical limit).
const UDF_MAX_ENTRY_SIZE: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB per file

// ============================================================================
// Path validation
// ============================================================================

fn validate_udf_entry_path(path: &Path) -> GeeZipResult<String> {
    let raw = path.to_str().ok_or_else(|| {
        GeeZipError::format(
            format!("non-UTF-8 path: {}", path.display()),
            ArchiveFormat::Udf,
        )
    })?;

    let normalized = raw.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');

    if normalized.is_empty() || normalized == "." || normalized == ".." {
        return Err(GeeZipError::format(
            format!("invalid UDF entry path: {raw}"),
            ArchiveFormat::Udf,
        ));
    }

    if is_entry_path_dangerous(Path::new(normalized)) {
        return Err(GeeZipError::format(
            format!("invalid UDF entry path: {raw}"),
            ArchiveFormat::Udf,
        ));
    }

    Ok(normalized.to_string())
}

// ============================================================================
// Reader
// ============================================================================

/// Seekable image source. Only tests keep images in memory.
enum UdfSource {
    File(std::path::PathBuf),
    #[cfg(test)]
    Memory(Vec<u8>),
}
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}
impl UdfSource {
    fn open(&self) -> GeeZipResult<Box<dyn ReadSeek + '_>> {
        match self {
            Self::File(path) => Ok(Box::new(BufReader::with_capacity(
                64 * 1024,
                fs::File::open(path)?,
            ))),
            #[cfg(test)]
            Self::Memory(data) => Ok(Box::new(Cursor::new(data.as_slice()))),
        }
    }
}

/// UDF image reader; only directory metadata stays resident.
pub struct UdfReader {
    data: UdfSource,
    /// Cached entries after parsing.
    entries_cache: Option<Vec<Entry>>,
    format: ArchiveFormat,
}

impl fmt::Debug for UdfReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdfReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl UdfReader {
    /// Create a UDF reader from a file path.
    pub fn new(path: impl AsRef<Path>) -> GeeZipResult<Self> {
        let path = path.as_ref();
        fs::File::open(path).map_err(|e| GeeZipError::io(e, "opening UDF image"))?;
        Ok(Self {
            data: UdfSource::File(path.to_path_buf()),
            entries_cache: None,
            format: ArchiveFormat::Udf,
        })
    }

    fn ensure_parsed(&mut self) -> GeeZipResult<()> {
        if self.entries_cache.is_some() {
            return Ok(());
        }

        let cursor = self.data.open()?;
        let udf = UdfFs::open(cursor).map_err(|err| convert_udf_error(err, "opening UDF image"))?;

        let root = udf
            .root_dir()
            .map_err(|err| convert_udf_error(err, "reading UDF root directory"))?;

        let mut entries = Vec::new();
        collect_udf_entries(
            &udf,
            &mut self.data.open()?,
            &root,
            "",
            &mut entries,
            &mut HashSet::new(),
        )?;
        self.entries_cache = Some(entries);
        Ok(())
    }

    /// Extract a file from the UDF image by navigating to its ICB.
    fn extract_file_internal(&self, entry_path: &str, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut reader = self.data.open()?;

        // Independent handles keep directory seeks separate from payload I/O.
        let udf = UdfFs::open(self.data.open()?)
            .map_err(|err| convert_udf_error(err, "re-opening UDF image for extraction"))?;

        let root = udf
            .root_dir()
            .map_err(|err| convert_udf_error(err, "reading root dir for extraction"))?;

        // Find the directory entry for the requested path
        let dir_entry = find_udf_entry(&udf, root, entry_path)?;

        if dir_entry.is_directory {
            return Ok(0);
        }

        // Read the file entry from the ICB.
        read_udf_file_data(&mut reader, &udf, &dir_entry, writer).map_err(|e| {
            GeeZipError::format(
                format!("extracting UDF file '{}': {}", entry_path, e),
                ArchiveFormat::Udf,
            )
        })
    }
}

/// Recursively collect entries from a UDF directory tree.
fn collect_udf_entries<R: Read + Seek>(
    udf: &UdfFs<R>,
    data: &mut dyn ReadSeek,
    dir: &hadris_udf::dir::UdfDir,
    parent: &str,
    out: &mut Vec<Entry>,
    visited: &mut HashSet<(u16, u32)>,
) -> GeeZipResult<()> {
    if parent.split('/').count() > 128 {
        return Err(convert_udf_error(
            "directory nesting too deep",
            "listing UDF",
        ));
    }
    for entry in dir.entries() {
        // Skip parent and self references
        if entry.is_parent() || entry.name().is_empty() {
            continue;
        }

        let path = if parent.is_empty() {
            entry.name().to_string()
        } else {
            format!("{}/{}", parent, entry.name())
        };

        let size = if entry.is_directory {
            0
        } else {
            let buffer = read_udf_file_entry(data, udf, entry)?;
            u64::from_le_bytes(buffer[56..64].try_into().unwrap())
        };
        out.push(Entry {
            path: path.clone(),
            size,
            compressed_size: size,
            crc32: None,
            modified: None,
            is_dir: entry.is_directory,
        });

        if entry.is_directory {
            let key = (entry.icb.partition_ref_num, entry.icb.logical_block_num);
            if !visited.insert(key) {
                return Err(convert_udf_error("cyclic directory ICB", "listing UDF"));
            }
            let child = udf
                .read_directory(&entry.icb)
                .map_err(|e| convert_udf_error(e, format!("reading UDF directory '{path}'")))?;
            collect_udf_entries(udf, data, &child, &path, out, visited)?;
        }
    }
    Ok(())
}

/// Resolve every path component so a nested lookup cannot select a root file
/// with the same basename.
fn find_udf_entry<R: Read + Seek>(
    udf: &UdfFs<R>,
    mut dir: hadris_udf::dir::UdfDir,
    path: &str,
) -> GeeZipResult<UdfDirEntry> {
    let missing = || GeeZipError::EntryNotFound {
        name: path.to_owned(),
    };
    let mut segments = path.split('/').peekable();
    while let Some(segment) = segments.next() {
        let entry = dir
            .entries()
            .find(|e| e.name() == segment)
            .cloned()
            .ok_or_else(missing)?;
        if segments.peek().is_none() {
            return Ok(entry);
        }
        if !entry.is_directory {
            return Err(missing());
        }
        dir = udf
            .read_directory(&entry.icb)
            .map_err(|e| convert_udf_error(e, format!("reading UDF directory '{segment}'")))?;
    }
    Err(missing())
}

/// Read file data from a UDF image by navigating to the file's ICB.
///
/// This manually parses the File Entry structure to find the allocation
/// descriptors and read the file data, because `hadris-udf` does not expose
/// a public file-reading API.
fn read_udf_file_entry<R: Read + Seek + ?Sized, S: Read + Seek>(
    reader: &mut R,
    udf: &UdfFs<S>,
    entry: &UdfDirEntry,
) -> GeeZipResult<Vec<u8>> {
    let info = udf.info();
    let block_size = info.block_size as u64;
    let partition_start = info.partition_start as u64;
    // The upstream directory parser uses fixed 2048-byte sectors. Reject
    // layouts it cannot navigate consistently before indexing the ICB.
    if block_size != 2048 || entry.icb.partition_ref_num != 0 {
        return Err(convert_udf_error(
            "unsupported block size or partition",
            "reading UDF ICB",
        ));
    }

    // Navigate to the ICB location
    let icb_block = entry.icb.logical_block_num as u64;
    let icb_sector = partition_start + icb_block;
    let icb_offset = icb_sector * block_size;

    reader
        .seek(SeekFrom::Start(icb_offset))
        .map_err(|e| GeeZipError::io(e, "seeking to UDF file ICB"))?;

    // Read the sector containing the File Entry
    let mut buffer = vec![0u8; block_size as usize];
    reader
        .read_exact(&mut buffer)
        .map_err(|e| GeeZipError::io(e, "reading UDF File Entry"))?;

    // Parse the Descriptor Tag (16 bytes)
    let tag_identifier = u16::from_le_bytes([buffer[0], buffer[1]]);
    if !matches!(tag_identifier, 261 | 266) {
        // 261 = File Entry tag
        return Err(GeeZipError::format(
            format!("expected UDF File Entry tag (261), got {}", tag_identifier),
            ArchiveFormat::Udf,
        ));
    }

    Ok(buffer)
}

/// Stream recorded extents, bound them against the image before reading, and
/// reject unsupported allocation layouts instead of interpreting them as data.
fn read_udf_file_data<R: Read + Seek, S: Read + Seek>(
    reader: &mut R,
    udf: &UdfFs<S>,
    entry: &UdfDirEntry,
    writer: &mut dyn Write,
) -> GeeZipResult<u64> {
    let buffer = read_udf_file_entry(reader, udf, entry)?;
    if buffer[27] != 5 {
        return Err(convert_udf_error(
            "unsupported file type",
            "reading UDF file",
        ));
    }
    let file_size = u64::from_le_bytes(buffer[56..64].try_into().unwrap());
    let allocation_type = u16::from_le_bytes([buffer[34], buffer[35]]) & 7;
    let tag_identifier = u16::from_le_bytes([buffer[0], buffer[1]]);
    let ea_length_offset = if tag_identifier == 261 { 168 } else { 208 };

    if ea_length_offset + 8 > buffer.len() {
        return Err(GeeZipError::format(
            "UDF File Entry too small".to_string(),
            ArchiveFormat::Udf,
        ));
    }

    let ea_length = u32::from_le_bytes([
        buffer[ea_length_offset],
        buffer[ea_length_offset + 1],
        buffer[ea_length_offset + 2],
        buffer[ea_length_offset + 3],
    ]) as usize;

    let ad_length = u32::from_le_bytes([
        buffer[ea_length_offset + 4],
        buffer[ea_length_offset + 5],
        buffer[ea_length_offset + 6],
        buffer[ea_length_offset + 7],
    ]) as usize;

    let ad_start = (ea_length_offset + 8)
        .checked_add(ea_length)
        .ok_or_else(|| convert_udf_error("allocation offset overflow", "reading UDF"))?;
    let ad_end = ad_start
        .checked_add(ad_length)
        .ok_or_else(|| convert_udf_error("allocation length overflow", "reading UDF"))?;

    if ad_start > buffer.len() || ad_end > buffer.len() {
        return Err(GeeZipError::format(
            "UDF allocation descriptors out of bounds".to_string(),
            ArchiveFormat::Udf,
        ));
    }

    let ad_data = &buffer[ad_start..ad_end];

    if allocation_type == 3 {
        let size = usize::try_from(file_size)
            .ok()
            .filter(|size| *size <= ad_data.len())
            .ok_or_else(|| convert_udf_error("truncated embedded file", "reading UDF"))?;
        writer.write_all(&ad_data[..size])?;
        return Ok(file_size);
    }
    let descriptor_size = match allocation_type {
        0 => 8,
        1 => 16,
        _ => {
            return Err(convert_udf_error(
                "unsupported allocation type",
                "reading UDF",
            ))
        }
    };
    if ad_data.len() % descriptor_size != 0 {
        return Err(convert_udf_error(
            "partial allocation descriptor",
            "reading UDF",
        ));
    }
    let block_size = u64::from(udf.info().block_size);
    let partition_start = u64::from(udf.info().partition_start);
    let image_len = reader.seek(SeekFrom::End(0))?;
    let mut remaining = file_size;
    for chunk in ad_data.chunks_exact(descriptor_size) {
        let raw_length = u32::from_le_bytes(chunk[..4].try_into().unwrap());
        let extent_kind = raw_length >> 30;
        let extent_length = u64::from(raw_length & 0x3fff_ffff);
        let extent_position = u32::from_le_bytes(chunk[4..8].try_into().unwrap());
        if extent_kind != 0 || (allocation_type == 1 && chunk[8..10] != [0, 0]) {
            return Err(convert_udf_error(
                "unsupported extent type or partition",
                "reading UDF",
            ));
        }
        let length = extent_length.min(remaining);
        let extent_offset = (partition_start + u64::from(extent_position)) * block_size;
        if extent_offset > image_len || extent_length > image_len - extent_offset {
            return Err(convert_udf_error(
                "file extent outside image",
                "reading UDF",
            ));
        }
        reader.seek(SeekFrom::Start(extent_offset))?;
        let written = std::io::copy(&mut reader.take(length), writer)?;
        if written != length {
            return Err(convert_udf_error("truncated file extent", "reading UDF"));
        }
        remaining -= written;
    }
    if remaining != 0 {
        return Err(convert_udf_error(
            "file extents shorter than information length",
            "reading UDF",
        ));
    }
    Ok(file_size)
}

impl ArchiveReader for UdfReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.ensure_parsed()?;
        Ok(self
            .entries_cache
            .as_ref()
            .expect("entries should be cached")
            .clone())
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.ensure_parsed()?;

        if entry.is_dir {
            return Ok(0);
        }

        let byte_count = self.extract_file_internal(&entry.path, writer)?;

        writer
            .flush()
            .map_err(|e| GeeZipError::io(e, "flushing after UDF extraction"))?;

        Ok(byte_count)
    }
}

// ============================================================================
// Writer
// ============================================================================

/// UDF disc image writer.
///
/// Entries are buffered in memory during `add_entry_from_reader` calls and
/// written in a single pass when `finish()` is called.
pub struct UdfWriter<W: Write + Send> {
    inner: Option<W>,
    entries: Vec<BufferedUdfEntry>,
    format: ArchiveFormat,
}

#[derive(Clone)]
struct BufferedUdfEntry {
    path: String,
    data: Vec<u8>,
    is_dir: bool,
}

impl<W: Write + Send> fmt::Debug for UdfWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdfWriter")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> UdfWriter<W> {
    /// Create a new UDF writer targeting the given output.
    pub fn new(writer: W) -> Self {
        Self {
            inner: Some(writer),
            entries: Vec::new(),
            format: ArchiveFormat::Udf,
        }
    }
} // impl UdfWriter

/// Build a `SimpleDir` tree from buffered entries.
fn build_simple_dir_from_entries(entries: &[BufferedUdfEntry]) -> GeeZipResult<SimpleDir> {
    let mut root = SimpleDir::new("");

    for entry in entries {
        if entry.is_dir {
            continue;
        }

        let mut components = entry.path.rsplitn(2, '/');
        let name = components.next().unwrap_or(&entry.path);
        let parent = components.next().unwrap_or("");
        ensure_simple_dir(&mut root, parent).add_file(SimpleFile::new(name, entry.data.clone()));
    }

    // Add empty directories
    for entry in entries {
        if !entry.is_dir {
            continue;
        }

        ensure_simple_dir(&mut root, &entry.path);
    }

    Ok(root)
}

fn ensure_simple_dir<'a>(mut dir: &'a mut SimpleDir, path: &str) -> &'a mut SimpleDir {
    for segment in path.split('/').filter(|s| !s.is_empty() && *s != ".") {
        let index = match dir.subdirs.iter().position(|child| child.name == segment) {
            Some(index) => index,
            None => {
                dir.add_dir(SimpleDir::new(segment));
                dir.subdirs.len() - 1
            }
        };
        dir = &mut dir.subdirs[index];
    }
    dir
}

impl<W: Write + Send> ArchiveWriter for UdfWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let udf_path = validate_udf_entry_path(path)?;

        let mut data = Vec::with_capacity(65536);
        let mut chunk = [0u8; 65536];
        loop {
            let n = reader.read(&mut chunk).map_err(|e| {
                GeeZipError::io(e, format!("reading data for UDF entry '{udf_path}'"))
            })?;
            if n == 0 {
                break;
            }
            if data.len() as u64 + n as u64 > UDF_MAX_ENTRY_SIZE {
                return Err(GeeZipError::format(
                    format!("UDF entry '{}' exceeds size limit", udf_path),
                    ArchiveFormat::Udf,
                ));
            }
            data.extend_from_slice(&chunk[..n]);
        }

        self.entries.push(BufferedUdfEntry {
            path: udf_path,
            data,
            is_dir: false,
        });
        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let udf_path = validate_udf_entry_path(path)?;
        self.entries.push(BufferedUdfEntry {
            path: udf_path,
            data: Vec::new(),
            is_dir: true,
        });
        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let this = *self;
        let UdfWriter {
            inner,
            entries,
            format: _,
        } = this;

        let mut writer = inner.ok_or_else(|| {
            GeeZipError::format("UDF writer already finalised", ArchiveFormat::Udf)
        })?;

        if entries.is_empty() {
            return Err(GeeZipError::format(
                "cannot create an empty UDF image",
                ArchiveFormat::Udf,
            ));
        }

        let root = build_simple_dir_from_entries(&entries)?;

        let options = UdfWriteOptions {
            volume_id: "GEEZIPX".to_string(),
            ..Default::default()
        };

        // The formatter seeks between metadata and file extents. Its final
        // cursor position can precede the last directory, so use its reported
        // sector count and a growable buffer rather than truncating at position().
        let mut cursor = Cursor::new(Vec::new());
        let sectors = HadrisUdfWriter::format(&mut cursor, &root, options).map_err(|err| {
            GeeZipError::format(format!("formatting UDF image: {err}"), ArchiveFormat::Udf)
        })?;
        let written = u64::from(sectors) * 2048;
        let length = usize::try_from(written)
            .map_err(|e| convert_udf_error(e, "UDF image exceeds address space"))?;
        let mut udf_data = cursor.into_inner();
        udf_data.resize(length, 0);

        writer
            .write_all(&udf_data)
            .map_err(|e| GeeZipError::io(e, "writing UDF image"))?;
        writer
            .flush()
            .map_err(|e| GeeZipError::io(e, "flushing UDF image"))?;

        Ok(written)
    }
}

// ============================================================================
// Error conversion
// ============================================================================

fn convert_udf_error(err: impl std::fmt::Display, context: impl Into<String>) -> GeeZipError {
    GeeZipError::format(format!("{}: {}", context.into(), err), ArchiveFormat::Udf)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    // use std::path::PathBuf;

    /// Build a minimal valid UDF image for testing.
    fn build_test_udf() -> Vec<u8> {
        let mut root = SimpleDir::new("");
        root.add_file(SimpleFile::new("hello.txt", b"hello udf\n".to_vec()));

        let estimate = 1024 * 1024; // 1 MB
        let mut buf = vec![0u8; estimate];
        let mut cursor = Cursor::new(&mut buf[..]);

        let options = UdfWriteOptions {
            volume_id: "TEST_UDF".to_string(),
            ..Default::default()
        };

        HadrisUdfWriter::format(&mut cursor, &root, options).expect("test UDF creation failed");

        let len = cursor.position() as usize;
        buf[..len].to_vec()
    }

    /// Build a UDF image with nested directories.
    #[allow(dead_code)]
    fn build_test_udf_nested() -> Vec<u8> {
        let mut subdir = SimpleDir::new("subdir");
        subdir.add_file(SimpleFile::new("nested.txt", b"nested\n".to_vec()));

        let mut root = SimpleDir::new("");
        root.add_file(SimpleFile::new("hello.txt", b"hello\n".to_vec()));
        root.add_dir(subdir);

        let estimate = 2 * 1024 * 1024;
        let mut buf = vec![0u8; estimate];
        let mut cursor = Cursor::new(&mut buf[..]);

        let options = UdfWriteOptions {
            volume_id: "TEST_NESTED".to_string(),
            ..Default::default()
        };

        HadrisUdfWriter::format(&mut cursor, &root, options).expect("test UDF creation failed");

        let len = cursor.position() as usize;
        buf[..len].to_vec()
    }

    #[test]
    fn udf_sparse_image_reads_without_loading_padding() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&build_test_udf()).unwrap();
        file.as_file().set_len(8 * 1024 * 1024 * 1024).unwrap();
        let mut reader = UdfReader::new(file.path()).unwrap();
        let entry = reader
            .entries()
            .unwrap()
            .into_iter()
            .find(|e| e.path == "hello.txt")
            .unwrap();
        let mut data = Vec::new();
        reader.extract(&entry, &mut data).unwrap();
        assert_eq!(data, b"hello udf\n");
    }

    #[test]
    fn udf_entries_and_extract_file() {
        let data = build_test_udf();
        let mut reader = UdfReader {
            data: UdfSource::Memory(data),
            entries_cache: None,
            format: ArchiveFormat::Udf,
        };
        let entries = reader.entries().expect("UDF entries should parse");
        assert!(!entries.is_empty(), "should have entries");

        let hello = entries
            .iter()
            .find(|e| e.path == "hello.txt")
            .expect("hello.txt should exist")
            .clone();
        assert!(!hello.is_dir);

        let mut out = Vec::new();
        let bytes = reader
            .extract(&hello, &mut out)
            .expect("hello.txt should extract");
        assert_eq!(bytes, b"hello udf\n".len() as u64);
        assert_eq!(out, b"hello udf\n");
    }

    #[test]
    fn udf_lists_and_extracts_nested_entries() {
        let mut reader = UdfReader {
            data: UdfSource::Memory(build_test_udf_nested()),
            entries_cache: None,
            format: ArchiveFormat::Udf,
        };
        let entries = reader.entries().unwrap();
        let entry = entries
            .iter()
            .find(|e| e.path == "subdir/nested.txt")
            .unwrap();
        assert_eq!(entry.size, 7);
        let mut output = Vec::new();
        assert_eq!(reader.extract(entry, &mut output).unwrap(), 7);
        assert_eq!(output, b"nested\n");
        let mut missing = entries
            .iter()
            .find(|e| e.path == "hello.txt")
            .unwrap()
            .clone();
        missing.path = "missing/hello.txt".into();
        assert!(matches!(
            reader.extract(&missing, &mut Vec::new()),
            Err(GeeZipError::EntryNotFound { .. })
        ));
    }

    #[test]
    fn udf_writer_preserves_nested_paths_and_empty_directories() {
        let mut data = Vec::new();
        let mut writer = Box::new(UdfWriter::new(&mut data));
        for (path, content) in [
            ("same.txt", b"root".as_slice()),
            ("a/b/same.txt", b"nested"),
        ] {
            writer
                .add_entry_from_reader(Path::new(path), &mut Cursor::new(content))
                .unwrap();
        }
        // Explicit directories may arrive after their children.
        writer.add_directory(Path::new("a")).unwrap();
        writer.add_directory(Path::new("a/b")).unwrap();
        writer.add_directory(Path::new("a/empty/deep")).unwrap();
        writer.finish().unwrap();
        let mut reader = UdfReader {
            data: UdfSource::Memory(data),
            entries_cache: None,
            format: ArchiveFormat::Udf,
        };
        let entries = reader.entries().unwrap();
        assert_eq!(entries.iter().filter(|e| e.path == "a").count(), 1);
        assert!(entries.iter().any(|e| e.path == "a/empty/deep" && e.is_dir));
        for (path, content) in [
            ("same.txt", b"root".as_slice()),
            ("a/b/same.txt", b"nested"),
        ] {
            let entry = entries.iter().find(|e| e.path == path).unwrap();
            let mut output = Vec::new();
            reader.extract(entry, &mut output).unwrap();
            assert_eq!(output, content);
        }
    }

    fn first_file_icb_offset(data: &[u8]) -> usize {
        let fs = UdfFs::open(Cursor::new(data)).unwrap();
        let dir = fs.root_dir().unwrap();
        let entry = dir.entries().find(|entry| !entry.is_directory).unwrap();
        (fs.info().partition_start as usize + entry.icb.logical_block_num as usize) * 2048
    }

    #[test]
    fn udf_rejects_invalid_extents_without_large_allocation() {
        for length in [0xffff_ffff_u32, 0x3fff_ffff, 1] {
            let mut data = build_test_udf();
            let offset = first_file_icb_offset(&data);
            data[offset + 176..offset + 180].copy_from_slice(&length.to_le_bytes());
            let mut reader = UdfReader {
                data: UdfSource::Memory(data),
                entries_cache: None,
                format: ArchiveFormat::Udf,
            };
            let entry = reader.entries().unwrap().remove(0);
            assert!(reader.extract(&entry, &mut Vec::new()).is_err());
        }
    }

    #[test]
    fn udf_reads_embedded_and_long_allocations_and_trims_padding() {
        for embedded in [true, false] {
            let mut data = build_test_udf();
            let offset = first_file_icb_offset(&data);
            if embedded {
                data[offset + 34..offset + 36].copy_from_slice(&3_u16.to_le_bytes());
                data[offset + 56..offset + 64].copy_from_slice(&3_u64.to_le_bytes());
                data[offset + 172..offset + 176].copy_from_slice(&3_u32.to_le_bytes());
                data[offset + 176..offset + 179].copy_from_slice(b"abc");
            } else {
                data[offset + 34..offset + 36].copy_from_slice(&1_u16.to_le_bytes());
                data[offset + 172..offset + 176].copy_from_slice(&16_u32.to_le_bytes());
                data[offset + 176..offset + 180].copy_from_slice(&2048_u32.to_le_bytes());
                data[offset + 184..offset + 192].fill(0);
            }
            let mut reader = UdfReader {
                data: UdfSource::Memory(data),
                entries_cache: None,
                format: ArchiveFormat::Udf,
            };
            let entry = reader.entries().unwrap().remove(0);
            let mut output = Vec::new();
            reader.extract(&entry, &mut output).unwrap();
            assert_eq!(
                output,
                if embedded {
                    b"abc".as_slice()
                } else {
                    b"hello udf\n"
                }
            );
            assert_eq!(output.len() as u64, entry.size);
        }
    }

    #[test]
    fn udf_invalid_image_fails_cleanly() {
        let mut reader = UdfReader {
            data: UdfSource::Memory(b"not a udf image".to_vec()),
            entries_cache: None,
            format: ArchiveFormat::Udf,
        };
        let err = reader.entries().expect_err("invalid UDF should fail");
        assert!(
            err.to_string().contains("udf") || err.to_string().contains("UDF"),
            "error should mention UDF: {err}"
        );
    }

    #[test]
    fn udf_writer_single_file_roundtrip() {
        let content = b"hello udf writer";
        let mut writer: Box<dyn ArchiveWriter> = Box::new(UdfWriter::new(Cursor::new(Vec::new())));
        writer
            .add_entry_from_reader(Path::new("HELLO.TXT"), &mut Cursor::new(content.to_vec()))
            .expect("file should be added");

        let bytes_written = writer.finish().expect("writer should finish");
        assert!(bytes_written > 0, "should have written data");
    }

    #[test]
    fn udf_writer_rejects_absolute_paths() {
        let mut writer: Box<dyn ArchiveWriter> = Box::new(UdfWriter::new(Cursor::new(Vec::new())));
        let err = writer
            .add_entry_from_reader(Path::new("/etc/passwd"), &mut Cursor::new(b"bad"))
            .expect_err("absolute path should be rejected");
        assert!(err.to_string().contains("invalid"), "error: {err}");
    }

    #[test]
    fn udf_writer_rejects_traversal_paths() {
        let mut writer: Box<dyn ArchiveWriter> = Box::new(UdfWriter::new(Cursor::new(Vec::new())));
        let err = writer
            .add_entry_from_reader(Path::new("../evil.txt"), &mut Cursor::new(b"bad"))
            .expect_err("traversal path should be rejected");
        assert!(err.to_string().contains("invalid"), "error: {err}");
    }

    #[test]
    fn udf_writer_rejects_invalid_path_before_reading_payload() {
        struct PanicReader;
        impl Read for PanicReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                panic!("should not read payload for invalid path");
            }
        }

        let mut writer: Box<dyn ArchiveWriter> = Box::new(UdfWriter::new(Cursor::new(Vec::new())));
        let err = writer
            .add_entry_from_reader(Path::new("../evil.txt"), &mut PanicReader)
            .expect_err("invalid path should fail before reading payload");
        assert!(err.to_string().contains("invalid"), "error: {err}");
    }
}
