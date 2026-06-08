//! Single-stream bzip2 compression and decompression helpers.
//!
//! These functions work on a single byte stream — they do **not**
//! implement [`ArchiveReader`] or [`ArchiveWriter`] because bzip2 is a
//! compression format, not an archive container. Use [`TarBz2Reader`] /
//! [`TarBz2Writer`] for tar.bz2 archives.
//!
//! [`ArchiveReader`]: super::ArchiveReader
//! [`ArchiveWriter`]: super::ArchiveWriter
//! [`TarBz2Reader`]: super::tarbz2::TarBz2Reader
//! [`TarBz2Writer`]: super::tarbz2::TarBz2Writer

use std::io::{Read, Write};

use crate::config::CompressOptions;
use crate::error::{GeeZipError, GeeZipResult};

/// Convert an optional compression level (0-9) to a `bzip2::Compression`.
///
/// `None` and `Some(0)` map to the default level (6). Unlike gzip/xz, the
/// libbz2 API does not support a true "store only" level 0 mode.
fn level_to_compression(level: Option<u32>) -> ::bzip2::Compression {
    match level {
        None | Some(0) => ::bzip2::Compression::default(),
        Some(l @ 1..=9) => ::bzip2::Compression::new(l),
        Some(l) => panic!("expected bzip2 compression level in 0..=9, got {l}"),
    }
}

/// Compress data from `reader` into `writer` using bzip2 at the given level.
///
/// `level` controls the bzip2 compression strength:
/// - `None`: use the default level (6).
/// - `Some(0)`: also use the default level (libbz2 has no store-only mode).
/// - `Some(1)`: fastest compression.
/// - `Some(6)`: default (good balance).
/// - `Some(9)`: best compression ratio (slowest).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn bzip2_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    let compression = level_to_compression(level);
    let mut encoder = ::bzip2::write::BzEncoder::new(writer, compression);
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "bzip2 compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e, "bzip2 compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using bzip2 with full options.
///
/// Currently only `options.level` is applied; `options.jobs` is accepted
/// but ignored because the bzip2 crate does not expose a stable multi-threaded
/// encoder API.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn bzip2_compress_with_options<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
) -> GeeZipResult<u64> {
    bzip2_compress_with_level(reader, writer, options.level)
}

/// Compress data from `reader` into `writer` using bzip2 with the default level.
///
/// This is a convenience wrapper around [`bzip2_compress_with_level`] with
/// `level: None` (default compression).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn bzip2_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    bzip2_compress_with_level(reader, writer, None)
}

/// Decompress bzip2-compressed data from `reader` into `writer`.
///
/// Uses `MultiBzDecoder` so concatenated bzip2 streams are handled correctly.
///
/// Returns the number of bytes written to the output (decompressed size).
pub fn bzip2_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = ::bzip2::read::MultiBzDecoder::new(reader);
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "bzip2 decompression failed"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bzip2_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of bzip2 compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            bzip2_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        assert_eq!(&compressed[..3], b"BZh", "bzip2 magic expected");

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = bzip2_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn bzip2_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            bzip2_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce bzip2 stream"
        );
        assert_eq!(&compressed[..3], b"BZh", "bzip2 magic expected");

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = bzip2_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn bzip2_corrupted_data_fails() {
        let bad_data = b"this is not bzip2 data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = bzip2_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("bzip2")
                || msg.contains("bz")
                || msg.contains("io")
                || msg.contains("invalid"),
            "expected bzip2/io error, got: {err}"
        );
    }

    #[test]
    fn bzip2_truncated_data_fails() {
        let mut source = Cursor::new(b"truncated bzip2 test payload".as_slice());
        let mut compressed = {
            let mut buf = Vec::new();
            bzip2_compress(&mut source, &mut buf).unwrap();
            buf
        };
        compressed.truncate(compressed.len().saturating_sub(4));

        let mut reader = Cursor::new(compressed.as_slice());
        let mut output = Vec::new();
        let err = bzip2_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("bzip2")
                || msg.contains("bz")
                || msg.contains("io")
                || msg.contains("unexpected"),
            "expected truncated bzip2 error, got: {err}"
        );
    }

    #[test]
    fn bzip2_with_level_9() {
        let original = b"Hello, GeeZipX! Level 9 bzip2 compression test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            bzip2_compress_with_level(&mut source, &mut buf, Some(9)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(&compressed[..3], b"BZh", "bzip2 magic expected");

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        bzip2_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn bzip2_with_level_1() {
        let original = b"Hello, GeeZipX! Level 1 bzip2 compression test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            bzip2_compress_with_level(&mut source, &mut buf, Some(1)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(&compressed[..3], b"BZh", "bzip2 magic expected");

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        bzip2_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn bzip2_level_0_falls_back_to_default() {
        let original = b"GeeZipX bzip2 level 0 = default test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed_level0 = {
            let mut buf = Vec::new();
            bzip2_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        source.set_position(0);
        let compressed_default = {
            let mut buf = Vec::new();
            bzip2_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(!compressed_level0.is_empty());
        assert!(!compressed_default.is_empty());
        assert_eq!(&compressed_level0[..3], b"BZh");
        assert_eq!(&compressed_default[..3], b"BZh");

        let mut out1 = Vec::new();
        let mut reader1 = Cursor::new(compressed_level0.as_slice());
        bzip2_decompress(&mut reader1, &mut out1).unwrap();
        assert_eq!(out1, original);

        let mut out2 = Vec::new();
        let mut reader2 = Cursor::new(compressed_default.as_slice());
        bzip2_decompress(&mut reader2, &mut out2).unwrap();
        assert_eq!(out2, original);
    }
}
