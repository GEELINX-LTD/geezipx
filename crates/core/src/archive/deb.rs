//! Debian package (`.deb`) archive reader.
//!
//! GeeZipX treats a DEB as a read-only container backed by an outer Unix `ar`
//! archive whose useful payload lives in a single `data.tar*` member.
//! `debian-binary` and `control.tar.*` are intentionally ignored in this
//! phase so the behavior matches `dpkg-deb -x/-c` style payload extraction.

use std::fmt;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Read-only Debian package reader.
pub struct DebReader<R: Read + Seek + Send> {
    inner: R,
    format: ArchiveFormat,
}

impl<R: Read + Seek + Send> fmt::Debug for DebReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DebReader")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl<R: Read + Seek + Send> DebReader<R> {
    /// Create a DEB reader from any `Read + Seek + Send` source.
    pub fn new(reader: R) -> Self {
        DebReader {
            inner: reader,
            format: ArchiveFormat::Deb,
        }
    }
}

impl DebReader<std::io::Cursor<Vec<u8>>> {
    /// Create a DEB reader from an already-loaded byte buffer.
    pub fn from_buf(buf: Vec<u8>) -> Self {
        DebReader::new(std::io::Cursor::new(buf))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataPayloadCodec {
    Tar,
    Gzip,
    Xz,
    Zstd,
    Bzip2,
    Lzma,
}

impl DataPayloadCodec {
    fn member_name(self) -> &'static str {
        match self {
            DataPayloadCodec::Tar => "data.tar",
            DataPayloadCodec::Gzip => "data.tar.gz",
            DataPayloadCodec::Xz => "data.tar.xz",
            DataPayloadCodec::Zstd => "data.tar.zst",
            DataPayloadCodec::Bzip2 => "data.tar.bz2",
            DataPayloadCodec::Lzma => "data.tar.lzma",
        }
    }
}

enum DataMemberKind {
    Supported(DataPayloadCodec),
    Unsupported(String),
    Ignore,
}

fn classify_data_member(name: &str) -> DataMemberKind {
    match name {
        "data.tar" => DataMemberKind::Supported(DataPayloadCodec::Tar),
        "data.tar.gz" => DataMemberKind::Supported(DataPayloadCodec::Gzip),
        "data.tar.xz" => DataMemberKind::Supported(DataPayloadCodec::Xz),
        "data.tar.zst" => DataMemberKind::Supported(DataPayloadCodec::Zstd),
        "data.tar.bz2" => DataMemberKind::Supported(DataPayloadCodec::Bzip2),
        "data.tar.lzma" => DataMemberKind::Supported(DataPayloadCodec::Lzma),
        other if other.starts_with("data.tar") => DataMemberKind::Unsupported(other.to_owned()),
        _ => DataMemberKind::Ignore,
    }
}

fn normalize_ar_identifier(raw: &[u8]) -> GeeZipResult<String> {
    let name = std::str::from_utf8(raw).map_err(|e| {
        GeeZipError::format(
            format!("non-UTF-8 DEB ar member name: {e}"),
            ArchiveFormat::Deb,
        )
    })?;
    Ok(name.trim_end().trim_end_matches('/').to_owned())
}

fn unsupported_data_payload(name: &str) -> GeeZipError {
    GeeZipError::format(
        format!(
            "unsupported DEB data payload '{name}' (supported: data.tar, data.tar.gz, data.tar.xz, data.tar.zst, data.tar.bz2, data.tar.lzma)"
        ),
        ArchiveFormat::Deb,
    )
}

fn convert_deb_ar_error(err: std::io::Error) -> GeeZipError {
    match err.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
            GeeZipError::format(format!("invalid DEB ar archive: {err}"), ArchiveFormat::Deb)
        }
        _ => GeeZipError::io(err, "reading DEB ar archive"),
    }
}

fn convert_deb_payload_error(err: std::io::Error, member: &str) -> GeeZipError {
    match err.kind() {
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => GeeZipError::format(
            format!("invalid DEB payload '{member}': {err}"),
            ArchiveFormat::Deb,
        ),
        _ => GeeZipError::io(err, format!("reading DEB payload '{member}'")),
    }
}

fn collect_deb_tar_entries<R: Read>(
    member: &str,
    archive: &mut tar::Archive<R>,
) -> GeeZipResult<Vec<Entry>> {
    let mut entries = Vec::new();

    for result in archive
        .entries()
        .map_err(|e| convert_deb_payload_error(e, member))?
    {
        let tar_entry = result.map_err(|e| convert_deb_payload_error(e, member))?;
        let header = tar_entry.header();
        let entry_type = header.entry_type();
        if matches!(
            entry_type,
            tar::EntryType::XGlobalHeader
                | tar::EntryType::GNULongLink
                | tar::EntryType::GNULongName
        ) {
            continue;
        }

        let path = tar_entry
            .path()
            .map_err(|e| convert_deb_payload_error(e, member))?
            .to_string_lossy()
            .into_owned();
        let size = tar_entry.size();
        let is_dir = entry_type.is_dir();

        entries.push(Entry {
            path,
            size,
            compressed_size: 0,
            crc32: None,
            modified: header.mtime().ok(),
            is_dir,
        });
    }

    Ok(entries)
}

fn extract_deb_tar_entry<R: Read>(
    member: &str,
    archive: &mut tar::Archive<R>,
    entry: &Entry,
    writer: &mut dyn Write,
) -> GeeZipResult<u64> {
    for result in archive
        .entries()
        .map_err(|e| convert_deb_payload_error(e, member))?
    {
        let mut tar_entry = result.map_err(|e| convert_deb_payload_error(e, member))?;
        let path = tar_entry
            .path()
            .map_err(|e| convert_deb_payload_error(e, member))?
            .to_string_lossy()
            .into_owned();

        if path == entry.path {
            if tar_entry.header().entry_type().is_dir() {
                return Ok(0);
            }
            let bytes = std::io::copy(&mut tar_entry, writer).map_err(|e| {
                GeeZipError::io(
                    e,
                    format!("extracting '{}' from DEB payload '{member}'", entry.path),
                )
            })?;
            return Ok(bytes);
        }
    }

    Err(GeeZipError::EntryNotFound {
        name: entry.path.clone(),
    })
}

fn collect_entries_from_member<R: Read>(
    member: ar::Entry<R>,
    codec: DataPayloadCodec,
) -> GeeZipResult<Vec<Entry>> {
    let member_name = codec.member_name();
    match codec {
        DataPayloadCodec::Tar => {
            let mut archive = tar::Archive::new(member);
            collect_deb_tar_entries(member_name, &mut archive)
        }
        DataPayloadCodec::Gzip => {
            let decoder = flate2::read::MultiGzDecoder::new(member);
            let mut archive = tar::Archive::new(decoder);
            collect_deb_tar_entries(member_name, &mut archive)
        }
        DataPayloadCodec::Xz => {
            let decoder = xz2::read::XzDecoder::new_multi_decoder(member);
            let mut archive = tar::Archive::new(decoder);
            collect_deb_tar_entries(member_name, &mut archive)
        }
        DataPayloadCodec::Zstd => {
            let decoder = zstd::stream::read::Decoder::new(member).map_err(|e| {
                GeeZipError::io(
                    e,
                    format!("initializing DEB payload '{member_name}' decoder"),
                )
            })?;
            let mut archive = tar::Archive::new(decoder);
            collect_deb_tar_entries(member_name, &mut archive)
        }
        DataPayloadCodec::Bzip2 => {
            let decoder = ::bzip2::read::MultiBzDecoder::new(member);
            let mut archive = tar::Archive::new(decoder);
            collect_deb_tar_entries(member_name, &mut archive)
        }
        DataPayloadCodec::Lzma => {
            let stream = xz2::stream::Stream::new_lzma_decoder(u64::MAX).map_err(|e| {
                GeeZipError::io(
                    e.into(),
                    format!("initializing DEB payload '{member_name}' decoder"),
                )
            })?;
            let decoder = xz2::read::XzDecoder::new_stream(member, stream);
            let mut archive = tar::Archive::new(decoder);
            collect_deb_tar_entries(member_name, &mut archive)
        }
    }
}

fn extract_from_member<R: Read>(
    member: ar::Entry<R>,
    codec: DataPayloadCodec,
    entry: &Entry,
    writer: &mut dyn Write,
) -> GeeZipResult<u64> {
    let member_name = codec.member_name();
    match codec {
        DataPayloadCodec::Tar => {
            let mut archive = tar::Archive::new(member);
            extract_deb_tar_entry(member_name, &mut archive, entry, writer)
        }
        DataPayloadCodec::Gzip => {
            let decoder = flate2::read::MultiGzDecoder::new(member);
            let mut archive = tar::Archive::new(decoder);
            extract_deb_tar_entry(member_name, &mut archive, entry, writer)
        }
        DataPayloadCodec::Xz => {
            let decoder = xz2::read::XzDecoder::new_multi_decoder(member);
            let mut archive = tar::Archive::new(decoder);
            extract_deb_tar_entry(member_name, &mut archive, entry, writer)
        }
        DataPayloadCodec::Zstd => {
            let decoder = zstd::stream::read::Decoder::new(member).map_err(|e| {
                GeeZipError::io(
                    e,
                    format!("initializing DEB payload '{member_name}' decoder"),
                )
            })?;
            let mut archive = tar::Archive::new(decoder);
            extract_deb_tar_entry(member_name, &mut archive, entry, writer)
        }
        DataPayloadCodec::Bzip2 => {
            let decoder = ::bzip2::read::MultiBzDecoder::new(member);
            let mut archive = tar::Archive::new(decoder);
            extract_deb_tar_entry(member_name, &mut archive, entry, writer)
        }
        DataPayloadCodec::Lzma => {
            let stream = xz2::stream::Stream::new_lzma_decoder(u64::MAX).map_err(|e| {
                GeeZipError::io(
                    e.into(),
                    format!("initializing DEB payload '{member_name}' decoder"),
                )
            })?;
            let decoder = xz2::read::XzDecoder::new_stream(member, stream);
            let mut archive = tar::Archive::new(decoder);
            extract_deb_tar_entry(member_name, &mut archive, entry, writer)
        }
    }
}

impl<R: Read + Seek + Send> ArchiveReader for DebReader<R> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.seek(SeekFrom::Start(0))?;
        let mut archive = ar::Archive::new(&mut self.inner);
        let mut unsupported = None;

        while let Some(result) = archive.next_entry() {
            let entry = result.map_err(convert_deb_ar_error)?;
            let member_name = normalize_ar_identifier(entry.header().identifier())?;
            match classify_data_member(&member_name) {
                DataMemberKind::Supported(codec) => {
                    return collect_entries_from_member(entry, codec)
                }
                DataMemberKind::Unsupported(name) => {
                    if unsupported.is_none() {
                        unsupported = Some(name);
                    }
                }
                DataMemberKind::Ignore => {}
            }
        }

        if let Some(name) = unsupported {
            Err(unsupported_data_payload(&name))
        } else {
            Ok(Vec::new())
        }
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        self.inner.seek(SeekFrom::Start(0))?;
        let mut archive = ar::Archive::new(&mut self.inner);
        let mut unsupported = None;

        while let Some(result) = archive.next_entry() {
            let member = result.map_err(convert_deb_ar_error)?;
            let member_name = normalize_ar_identifier(member.header().identifier())?;
            match classify_data_member(&member_name) {
                DataMemberKind::Supported(codec) => {
                    return extract_from_member(member, codec, entry, writer);
                }
                DataMemberKind::Unsupported(name) => {
                    if unsupported.is_none() {
                        unsupported = Some(name);
                    }
                }
                DataMemberKind::Ignore => {}
            }
        }

        if let Some(name) = unsupported {
            Err(unsupported_data_payload(&name))
        } else {
            Err(GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;
    use std::path::Path;

    fn create_test_tar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let mut builder = tar::Builder::new(buf);
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).unwrap();
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append(&header, std::io::Cursor::new(data)).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn create_raw_tar(path: &[u8], data: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        let name_len = path.len().min(99);
        header[..name_len].copy_from_slice(&path[..name_len]);
        header[100..108].copy_from_slice(b"0000644\0");
        let size_oct = format!("{:011o}\0", data.len());
        header[124..136].copy_from_slice(size_oct.as_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        for b in header.iter_mut().take(156).skip(148) {
            *b = b' ';
        }
        let cksum: u32 = header.iter().map(|&b| b as u32).sum();
        let cksum_str = format!("{:06o}\0 ", cksum);
        header[148..156].copy_from_slice(cksum_str.as_bytes());

        let mut archive = header.to_vec();
        if !data.is_empty() {
            archive.extend_from_slice(data);
            let padding = (512 - data.len() % 512) % 512;
            archive.extend(std::iter::repeat_n(0, padding));
        }
        archive.extend_from_slice(&[0u8; 1024]);
        archive
    }

    fn gzip_bytes(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
        out
    }

    fn xz_bytes(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut reader = Cursor::new(data);
        crate::archive::xz::xz_compress(&mut reader, &mut out).unwrap();
        out
    }

    fn zstd_bytes(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = zstd::stream::write::Encoder::new(&mut out, 0).unwrap();
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
        out
    }

    fn bzip2_bytes(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = ::bzip2::write::BzEncoder::new(&mut out, ::bzip2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
        out
    }

    fn lzma_bytes(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut reader = Cursor::new(data);
        crate::archive::xz::lzma_compress(&mut reader, &mut out).unwrap();
        out
    }

    fn append_ar_member(out: &mut Vec<u8>, name: &str, data: &[u8]) {
        assert!(name.len() <= 16, "DEB ar member name too long: {name}");
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8o}{:<10}`\n",
            name,
            0,
            0,
            0,
            0o100644,
            data.len()
        );
        assert_eq!(header.len(), 60, "invalid ar header length for {name}");
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            out.push(b'\n');
        }
    }

    fn build_deb(data_member: Option<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"!<arch>\n");
        append_ar_member(&mut out, "debian-binary", b"2.0\n");
        append_ar_member(&mut out, "control.tar.gz", b"ignored control payload");
        if let Some((name, data)) = data_member {
            append_ar_member(&mut out, name, &data);
        }
        out
    }

    fn sample_entries() -> [(&'static str, &'static [u8]); 2] {
        [
            ("usr/bin/hello", b"hello"),
            ("usr/share/doc/readme.txt", b"docs"),
        ]
    }

    fn sample_tar() -> Vec<u8> {
        create_test_tar(&sample_entries())
    }

    fn assert_roundtrip(member_name: &str, payload: Vec<u8>) {
        let deb = build_deb(Some((member_name, payload)));
        let mut reader = DebReader::from_buf(deb.clone());
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "usr/bin/hello");
        assert_eq!(entries[1].path, "usr/share/doc/readme.txt");

        let hello = entries
            .iter()
            .find(|entry| entry.path == "usr/bin/hello")
            .unwrap()
            .clone();
        let mut out = Vec::new();
        let bytes = reader.extract(&hello, &mut out).unwrap();
        assert_eq!(bytes, 5);
        assert_eq!(out, b"hello");

        let tmp = tempfile::tempdir().unwrap();
        let mut reader = DebReader::from_buf(deb);
        let report = reader.extract_all(tmp.path(), true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert_eq!(report.bytes_extracted, 9);
        assert!(report.errors.is_empty(), "report: {report:?}");
        assert_eq!(
            std::fs::read(tmp.path().join("usr/bin/hello")).unwrap(),
            b"hello"
        );
        assert_eq!(
            std::fs::read(tmp.path().join("usr/share/doc/readme.txt")).unwrap(),
            b"docs"
        );
    }

    #[test]
    fn deb_entries_plain_data_tar() {
        assert_roundtrip("data.tar", sample_tar());
    }

    #[test]
    fn deb_entries_gzip_data_tar() {
        assert_roundtrip("data.tar.gz", gzip_bytes(&sample_tar()));
    }

    #[test]
    fn deb_entries_xz_data_tar() {
        assert_roundtrip("data.tar.xz", xz_bytes(&sample_tar()));
    }

    #[test]
    fn deb_entries_zstd_data_tar() {
        assert_roundtrip("data.tar.zst", zstd_bytes(&sample_tar()));
    }

    #[test]
    fn deb_entries_bzip2_data_tar() {
        assert_roundtrip("data.tar.bz2", bzip2_bytes(&sample_tar()));
    }

    #[test]
    fn deb_entries_lzma_data_tar() {
        assert_roundtrip("data.tar.lzma", lzma_bytes(&sample_tar()));
    }

    #[test]
    fn deb_extract_missing_entry_from_empty_package() {
        let mut reader = DebReader::from_buf(build_deb(None));
        let entries = reader.entries().unwrap();
        assert!(entries.is_empty());

        let fake = Entry {
            path: "missing.txt".into(),
            size: 0,
            compressed_size: 0,
            crc32: None,
            modified: None,
            is_dir: false,
        };
        let err = reader.extract(&fake, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, GeeZipError::EntryNotFound { .. }));
    }

    #[test]
    fn deb_malformed_ar_fails_cleanly() {
        let mut reader = DebReader::from_buf(b"!<arch>\ntruncated".to_vec());
        let err = reader.entries().unwrap_err();
        assert!(err.to_string().contains("invalid DEB ar archive"));
    }

    #[test]
    fn deb_unsupported_data_codec_fails_cleanly() {
        let mut reader =
            DebReader::from_buf(build_deb(Some(("data.tar.lz4", b"not supported".to_vec()))));
        let err = reader.entries().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unsupported DEB data payload"), "msg: {msg}");
        assert!(msg.contains("data.tar.lz4"), "msg: {msg}");
    }

    #[test]
    fn deb_path_traversal_is_blocked_on_extract_all() {
        let payload = create_raw_tar(b"../escape.txt", b"owned");
        let deb = build_deb(Some(("data.tar", payload)));
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("escape.txt");

        let mut reader = DebReader::from_buf(deb);
        let report = reader.extract_all(tmp.path(), true).unwrap();
        assert_eq!(report.files_extracted, 0);
        assert!(report
            .errors
            .iter()
            .any(|(_, err)| matches!(err, GeeZipError::PathTraversal { .. })));
        assert!(!outside.exists());
    }

    #[test]
    fn deb_trait_object_is_supported() {
        fn use_reader(_reader: &mut dyn ArchiveReader) {}

        let deb = build_deb(Some(("data.tar", sample_tar())));
        let mut reader = DebReader::from_buf(deb);
        use_reader(&mut reader);
    }

    #[test]
    fn deb_extract_all_nested_paths_creates_directories() {
        let deb = build_deb(Some(("data.tar", sample_tar())));
        let tmp = tempfile::tempdir().unwrap();
        let out_dir = tmp.path().join("out");

        let mut reader = DebReader::from_buf(deb);
        let report = reader.extract_all(&out_dir, true).unwrap();
        assert_eq!(report.files_extracted, 2);
        assert!(out_dir.join(Path::new("usr/bin")).is_dir());
        assert!(out_dir.join(Path::new("usr/share/doc")).is_dir());
    }
}
