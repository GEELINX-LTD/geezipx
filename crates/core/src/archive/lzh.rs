//! LZH/LHA (`.lzh`, `.lha`) archive reader.
//!
//! GeeZipX exposes LZH as a read-only archive format backed by the `delharc`
//! decoder. The current MVP supports listing, extraction, and integrity
//! verification for the compression methods handled by `delharc`'s default
//! feature set (notably `-lh0-`, `-lh1-`, and `-lh4-` through `-lh7-`).
//!
//! Path handling notes:
//! - `delharc`'s parsed pathname helpers intentionally normalise separators and
//!   strip `.` / `..` / empty components. GeeZipX therefore inspects the raw
//!   filename and extended path headers first so dangerous names are surfaced
//!   as path-traversal errors instead of being silently renamed on extraction.
//! - Safe paths still use `delharc`'s parsed pathname so non-ASCII and control
//!   bytes are rendered consistently with the upstream decoder.

use std::fmt::{self, Write as _};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use delharc::header::{ext::EXT_HEADER_FILENAME, ext::EXT_HEADER_PATH, LhaHeader, OsType};
use delharc::{LhaDecodeReader, LhaError};

use crate::archive::{is_entry_path_dangerous, ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only LZH/LHA archive reader.
pub struct LzhReader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for LzhReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LzhReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> LzhReader<R> {
    /// Create an LZH reader from any `Read + Seek + Send` source.
    pub fn new(reader: R) -> Self {
        Self {
            inner: reader,
            format: ArchiveFormat::Lzh,
        }
    }
}

impl LzhReader<std::io::Cursor<Vec<u8>>> {
    /// Create an LZH reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        Self::new(std::io::Cursor::new(buf))
    }
}

fn lzh_modified_timestamp(header: &LhaHeader) -> Option<u64> {
    header
        .parse_last_modified()
        .to_utc()
        .and_then(|dt| u64::try_from(dt.timestamp()).ok())
}

fn lzh_is_amiga_nilterm(header: &LhaHeader) -> bool {
    header.parse_os_type() == Ok(OsType::Amiga)
}

fn split_raw_lzh_path_components(data: &[u8], nilterm: bool) -> Vec<Vec<u8>> {
    let mut components = Vec::new();
    let mut current = Vec::new();

    for &byte in data {
        if nilterm && byte == 0 {
            break;
        }

        if matches!(byte, 0xFF | b'/' | b'\\') {
            components.push(std::mem::take(&mut current));
        } else {
            current.push(byte);
        }
    }

    components.push(current);
    components
}

fn trim_trailing_empty_raw_components(components: &mut Vec<Vec<u8>>) {
    while components
        .last()
        .is_some_and(|component| component.is_empty())
    {
        components.pop();
    }
}

fn append_raw_lzh_path_source(components: &mut Vec<Vec<u8>>, data: &[u8], nilterm: bool) {
    if !components.is_empty() {
        trim_trailing_empty_raw_components(components);
    }
    components.extend(split_raw_lzh_path_components(data, nilterm));
}

fn raw_lzh_path_components(header: &LhaHeader) -> Vec<Vec<u8>> {
    let nilterm = lzh_is_amiga_nilterm(header);
    let mut components = Vec::new();
    let mut raw_path_headers = Vec::new();
    let mut filename_header = None;

    for extra in header.iter_extra() {
        match extra {
            [EXT_HEADER_FILENAME, data @ ..] => filename_header = Some(data),
            [EXT_HEADER_PATH, data @ ..] => raw_path_headers.push(data),
            _ => {}
        }
    }

    for data in raw_path_headers {
        append_raw_lzh_path_source(&mut components, data, false);
    }

    if let Some(data) = filename_header {
        append_raw_lzh_path_source(&mut components, data, nilterm);
    } else if !header.filename.is_empty() {
        append_raw_lzh_path_source(&mut components, &header.filename, nilterm);
    }

    components
}

fn raw_lzh_component_display(component: &[u8]) -> String {
    let mut out = String::with_capacity(component.len());
    for &byte in component {
        match byte {
            0x20..=0x7E => out.push(byte as char),
            _ => {
                let _ = write!(out, "%{:02x}", byte);
            }
        }
    }
    out
}

fn raw_lzh_display_path(components: &[Vec<u8>]) -> String {
    if components.is_empty() {
        return String::new();
    }

    let mut display = String::new();
    for (index, component) in components.iter().enumerate() {
        if index > 0 {
            display.push('/');
        }
        display.push_str(&raw_lzh_component_display(component));
    }
    display
}

fn raw_lzh_path_is_dangerous(components: &[Vec<u8>]) -> bool {
    let Some(last_index) = components.len().checked_sub(1) else {
        return true;
    };

    for (index, component) in components.iter().enumerate() {
        if component.is_empty() {
            if index == last_index {
                continue;
            }
            return true;
        }

        if component.as_slice() == b"." || component.as_slice() == b".." {
            return true;
        }

        if index == 0
            && component.len() >= 2
            && component[0].is_ascii_alphabetic()
            && component[1] == b':'
        {
            return true;
        }
    }

    false
}

fn normalize_lzh_display_path(mut path: String, is_dir: bool) -> String {
    if is_dir {
        while path.len() > 1 && path.ends_with('/') {
            path.pop();
        }
    }
    path
}

fn lzh_entry_path(header: &LhaHeader) -> GeeZipResult<String> {
    let raw_components = raw_lzh_path_components(header);
    let raw_display =
        normalize_lzh_display_path(raw_lzh_display_path(&raw_components), header.is_directory());
    if raw_display.is_empty() {
        return Err(GeeZipError::format(
            "LZH entry is missing a pathname",
            ArchiveFormat::Lzh,
        ));
    }

    if raw_lzh_path_is_dangerous(&raw_components) {
        return Ok(raw_display);
    }

    let path = normalize_lzh_display_path(
        header.parse_pathname_to_str().replace('\\', "/"),
        header.is_directory(),
    );
    if path.is_empty() {
        return Err(GeeZipError::format(
            "LZH entry is missing a pathname",
            ArchiveFormat::Lzh,
        ));
    }

    Ok(path)
}

fn compression_label(header: &LhaHeader) -> String {
    match header.compression_method() {
        Ok(method) => method.to_string(),
        Err(_) => String::from_utf8_lossy(&header.compression).into_owned(),
    }
}

fn unsupported_method_error(header: &LhaHeader, path: &str) -> GeeZipError {
    GeeZipError::format(
        format!(
            "unsupported LZH compression method '{}' for entry '{}'",
            compression_label(header),
            path
        ),
        ArchiveFormat::Lzh,
    )
}

fn convert_lha_error(err: LhaError<std::io::Error>, context: impl Into<String>) -> GeeZipError {
    match err {
        LhaError::Io(io_err) => match io_err.kind() {
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
                GeeZipError::format(format!("invalid LZH archive: {io_err}"), ArchiveFormat::Lzh)
            }
            _ => GeeZipError::io(io_err, context),
        },
        LhaError::HeaderParse(message) => GeeZipError::format(
            format!("invalid LZH archive: {message}"),
            ArchiveFormat::Lzh,
        ),
        LhaError::Decompress(message) => GeeZipError::format(
            format!("failed to decompress LZH entry: {message}"),
            ArchiveFormat::Lzh,
        ),
        LhaError::Checksum(message) => GeeZipError::format(
            format!("LZH CRC-16 verification failed: {message}"),
            ArchiveFormat::Lzh,
        ),
        _ => GeeZipError::format("LZH decoder error", ArchiveFormat::Lzh),
    }
}

fn convert_lha_decode_error<E>(err: E, context: impl Into<String>) -> GeeZipError
where
    E: Into<LhaError<std::io::Error>>,
{
    convert_lha_error(err.into(), context)
}

fn dangerous_lzh_entry_error(path: &str) -> GeeZipError {
    GeeZipError::PathTraversal {
        entry: path.to_string(),
        target: "requested extraction target".into(),
    }
}

fn scan_lzh_entries<R: Read + Seek + Send>(inner: &mut R) -> GeeZipResult<Vec<Entry>> {
    inner.seek(SeekFrom::Start(0))?;
    let mut reader = LhaDecodeReader::new(&mut *inner)
        .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?;
    let mut entries = Vec::new();

    loop {
        let header = reader.header();
        let path = lzh_entry_path(header)?;
        entries.push(Entry {
            path,
            size: header.original_size,
            compressed_size: header.compressed_size,
            crc32: None,
            modified: lzh_modified_timestamp(header),
            is_dir: header.is_directory(),
        });

        if !reader
            .next_file()
            .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?
        {
            break;
        }
    }

    Ok(entries)
}

impl<R: Read + Seek + Send> ArchiveReader for LzhReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        scan_lzh_entries(&mut self.inner)
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let mut reader = LhaDecodeReader::new(&mut self.inner)
            .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?;

        loop {
            let header = reader.header();
            let path = lzh_entry_path(header)?;
            if path == entry.path {
                let raw_components = raw_lzh_path_components(header);
                if raw_lzh_path_is_dangerous(&raw_components)
                    || is_entry_path_dangerous(Path::new(&path))
                {
                    return Err(dangerous_lzh_entry_error(&path));
                }
                if header.is_directory() {
                    return Ok(0);
                }
                if !reader.is_decoder_supported() {
                    return Err(unsupported_method_error(header, &path));
                }

                let bytes = std::io::copy(&mut reader, writer).map_err(|err| {
                    GeeZipError::io(err, format!("extracting '{}' from LZH archive", entry.path))
                })?;
                reader.crc_check().map_err(|err| {
                    convert_lha_error(
                        err,
                        format!("verifying CRC-16 for '{}' in LZH archive", entry.path),
                    )
                })?;
                return Ok(bytes);
            }

            if !reader
                .next_file()
                .map_err(|err| convert_lha_decode_error(err, "reading LZH archive"))?
            {
                break;
            }
        }

        Err(GeeZipError::EntryNotFound {
            name: entry.path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use delharc::crc::Crc16;
    use delharc::header::MsDosAttrs;

    struct FixtureEntry<'a> {
        path: &'a str,
        data: &'a [u8],
        method: [u8; 5],
    }

    impl<'a> FixtureEntry<'a> {
        fn file(path: &'a str, data: &'a [u8]) -> Self {
            Self {
                path,
                data,
                method: *b"-lh0-",
            }
        }

        fn directory(path: &'a str) -> Self {
            Self {
                path,
                data: &[],
                method: *b"-lhd-",
            }
        }

        fn unsupported(path: &'a str, data: &'a [u8]) -> Self {
            Self {
                path,
                data,
                method: *b"-pm1-",
            }
        }
    }

    fn file_crc16(data: &[u8]) -> u16 {
        let mut crc = Crc16::default();
        crc.digest(data);
        crc.sum16()
    }

    fn append_lzh_entry(out: &mut Vec<u8>, entry: &FixtureEntry<'_>) {
        let name = entry.path.as_bytes();
        assert!(name.len() <= u8::MAX as usize, "LZH pathname too long");
        let compressed_size = if entry.method == *b"-lhd-" {
            0u32
        } else {
            entry.data.len() as u32
        };
        let original_size = if entry.method == *b"-lhd-" {
            0u32
        } else {
            entry.data.len() as u32
        };
        let file_crc = if entry.method == *b"-lhd-" {
            0u16
        } else {
            file_crc16(entry.data)
        };
        let header_len = 22usize + name.len();
        assert!(header_len <= u8::MAX as usize, "LZH header too large");

        let mut header = Vec::with_capacity(header_len);
        header.extend_from_slice(&entry.method);
        header.extend_from_slice(&compressed_size.to_le_bytes());
        header.extend_from_slice(&original_size.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.push(0x20);
        header.push(0);
        header.push(name.len() as u8);
        header.extend_from_slice(name);
        header.extend_from_slice(&file_crc.to_le_bytes());
        assert_eq!(header.len(), header_len);

        let checksum = header.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        out.push(header_len as u8);
        out.push(checksum);
        out.extend_from_slice(&header);
        if entry.method != *b"-lhd-" {
            out.extend_from_slice(entry.data);
        }
    }

    fn build_lzh(entries: &[FixtureEntry<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        for entry in entries {
            append_lzh_entry(&mut out, entry);
        }
        out.push(0);
        out
    }

    fn append_lzh_level1_extra_header(
        out: &mut Vec<u8>,
        kind: u8,
        data: &[u8],
        next_header_len: u16,
    ) {
        let header_len = 1usize + data.len() + 2;
        assert!(
            u16::try_from(header_len).is_ok(),
            "LZH extra header too large"
        );
        out.push(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&next_header_len.to_le_bytes());
    }

    fn lzh_header_with_path_extra_headers(path_header: &[u8], filename_header: &[u8]) -> LhaHeader {
        let path_header_len =
            u16::try_from(1usize + path_header.len() + 2).expect("LZH path extra header too large");
        let filename_header_len = u16::try_from(1usize + filename_header.len() + 2)
            .expect("LZH filename extra header too large");

        let mut extra_headers = Vec::new();
        append_lzh_level1_extra_header(
            &mut extra_headers,
            EXT_HEADER_PATH,
            path_header,
            filename_header_len,
        );
        append_lzh_level1_extra_header(&mut extra_headers, EXT_HEADER_FILENAME, filename_header, 0);

        LhaHeader {
            level: 1,
            compression: *b"-lh0-",
            compressed_size: 0,
            original_size: 0,
            filename: Box::new([]),
            msdos_attrs: MsDosAttrs::ARCHIVE,
            last_modified: 0,
            os_type: 0,
            file_crc: 0,
            extended_area: Box::new([]),
            first_header_len: u32::from(path_header_len),
            extra_headers: extra_headers.into_boxed_slice(),
        }
    }

    fn assert_lzh_extra_header_path_rejected(
        path_header: &[u8],
        filename_header: &[u8],
        expected_path: &str,
    ) {
        let header = lzh_header_with_path_extra_headers(path_header, filename_header);
        let raw_components = raw_lzh_path_components(&header);
        assert!(raw_lzh_path_is_dangerous(&raw_components));
        assert_eq!(lzh_entry_path(&header).unwrap(), expected_path);
    }

    fn assert_lzh_extra_header_path_accepted(
        path_header: &[u8],
        filename_header: &[u8],
        expected_path: &str,
    ) {
        let header = lzh_header_with_path_extra_headers(path_header, filename_header);
        let raw_components = raw_lzh_path_components(&header);
        assert!(!raw_lzh_path_is_dangerous(&raw_components));
        assert_eq!(lzh_entry_path(&header).unwrap(), expected_path);
    }

    #[test]
    fn lzh_entries_normalize_paths_and_extract_files() {
        let archive = build_lzh(&[
            FixtureEntry::directory("docs\\"),
            FixtureEntry::file("docs\\hello.txt", b"hello"),
            FixtureEntry::file("readme.txt", b"readme"),
        ]);
        let mut reader = LzhReader::from_buf(archive);
        let entries = reader.entries().expect("entries should load");

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "docs");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].path, "docs/hello.txt");
        assert_eq!(entries[1].size, 5);
        assert!(!entries[1].is_dir);
        assert_eq!(entries[2].path, "readme.txt");

        let mut out = Vec::new();
        let bytes = reader
            .extract(&entries[1], &mut out)
            .expect("file extraction should succeed");
        assert_eq!(bytes, 5);
        assert_eq!(out, b"hello");
    }

    #[test]
    fn lzh_crc_mismatch_fails_cleanly() {
        let mut archive = build_lzh(&[FixtureEntry::file("hello.txt", b"hello")]);
        let payload_index = archive.len() - 1 - b"hello".len();
        archive[payload_index] ^= 0x01;

        let mut reader = LzhReader::from_buf(archive);
        let entry = reader.entries().unwrap().remove(0);
        let err = reader
            .extract(&entry, &mut std::io::sink())
            .expect_err("CRC mismatch should fail extraction");
        assert!(err.to_string().contains("CRC-16"), "err: {err}");
    }

    fn assert_lzh_extract_all_rejects_dangerous_path(raw_path: &str, expected_path: &str) {
        let archive = build_lzh(&[FixtureEntry::file(raw_path, b"bad")]);
        let mut reader = LzhReader::from_buf(archive);
        let entries = reader.entries().expect("entries should still be listable");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, expected_path);

        let temp = tempfile::TempDir::new().unwrap();
        let report = reader
            .extract_all(temp.path(), true)
            .expect("extract_all should report per-file errors");

        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, expected_path);
        assert!(matches!(
            report.errors[0].1,
            GeeZipError::PathTraversal { .. }
        ));
        assert!(!temp.path().join("evil.txt").exists());
    }

    #[test]
    fn lzh_extract_all_rejects_parent_dir_paths_from_raw_header() {
        assert_lzh_extract_all_rejects_dangerous_path("../evil.txt", "../evil.txt");
    }

    #[test]
    fn lzh_extract_all_rejects_unix_absolute_paths_from_raw_header() {
        assert_lzh_extract_all_rejects_dangerous_path("/absolute.txt", "/absolute.txt");
    }

    #[test]
    fn lzh_extract_all_rejects_windows_drive_paths_from_raw_header() {
        assert_lzh_extract_all_rejects_dangerous_path("C:\\evil.txt", "C:/evil.txt");
    }

    #[test]
    fn lzh_extract_all_rejects_backslash_traversal_from_raw_header() {
        assert_lzh_extract_all_rejects_dangerous_path("..\\evil.txt", "../evil.txt");
    }

    #[test]
    fn lzh_extract_all_rejects_unc_paths_from_raw_header() {
        assert_lzh_extract_all_rejects_dangerous_path(
            "\\\\server\\share\\evil.txt",
            "//server/share/evil.txt",
        );
    }

    #[test]
    fn lzh_extract_all_rejects_windows_drive_relative_paths_from_raw_header() {
        assert_lzh_extract_all_rejects_dangerous_path("C:evil.txt", "C:evil.txt");
    }

    #[test]
    fn lzh_extra_header_path_rejects_parent_dir_components() {
        assert_lzh_extra_header_path_rejected(b"../", b"evil.txt", "../evil.txt");
    }

    #[test]
    fn lzh_extra_header_path_rejects_absolute_components() {
        assert_lzh_extra_header_path_rejected(b"/abs/", b"evil.txt", "/abs/evil.txt");
    }

    #[test]
    fn lzh_extra_header_path_accepts_safe_components() {
        assert_lzh_extra_header_path_accepted(b"safe/", b"evil.txt", "safe/evil.txt");
    }

    #[test]
    fn lzh_extract_all_removes_partial_file_after_crc_failure() {
        let mut archive = build_lzh(&[FixtureEntry::file("hello.txt", b"hello")]);
        let payload_index = archive.len() - 1 - b"hello".len();
        archive[payload_index] ^= 0x01;

        let mut reader = LzhReader::from_buf(archive);
        let temp = tempfile::TempDir::new().unwrap();
        let report = reader.extract_all(temp.path(), true).unwrap();

        assert_eq!(report.files_extracted, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].1.to_string().contains("CRC-16"));
        assert!(!temp.path().join("hello.txt").exists());
    }

    #[test]
    fn lzh_unsupported_decoder_is_reported_structurally() {
        let archive = build_lzh(&[FixtureEntry::unsupported("legacy.bin", b"payload")]);
        let mut reader = LzhReader::from_buf(archive);
        let entry = reader.entries().unwrap().remove(0);
        let err = reader
            .extract(&entry, &mut std::io::sink())
            .expect_err("unsupported decoder should fail extraction");
        assert!(err
            .to_string()
            .contains("unsupported LZH compression method"));
        assert!(err.to_string().contains("-pm1-"));
    }

    #[test]
    fn lzh_trait_object_is_supported() {
        let archive = build_lzh(&[FixtureEntry::file("hello.txt", b"hello")]);
        let mut reader: Box<dyn ArchiveReader> = Box::new(LzhReader::from_buf(archive));
        let entries = reader.entries().expect("trait object should list entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "hello.txt");
    }
}
