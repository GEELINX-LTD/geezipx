//! Single-stream LZ4 frame compression and decompression helpers.
//!
//! These functions operate on a single byte stream — they do **not** implement
//! [`ArchiveReader`] or [`ArchiveWriter`] because LZ4 is a compression format,
//! not an archive container. Use [`TarLz4Reader`] / [`TarLz4Writer`] for
//! `.tar.lz4` archives.
//!
//! [`ArchiveReader`]: super::ArchiveReader
//! [`ArchiveWriter`]: super::ArchiveWriter
//! [`TarLz4Reader`]: super::tarlz4::TarLz4Reader
//! [`TarLz4Writer`]: super::tarlz4::TarLz4Writer

use std::io::{Read, Write};

use crate::config::CompressOptions;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

fn validate_level(level: Option<u32>) -> GeeZipResult<()> {
    match level {
        None | Some(0) => Ok(()),
        Some(l) => Err(GeeZipError::format(
            format!(
                "lz4 compression level is not configurable in the current encoder; use 0 or omit the level (got {l})"
            ),
            ArchiveFormat::Lz4,
        )),
    }
}

/// Compress data from `reader` into `writer` using an LZ4 frame.
///
/// The selected pure-Rust encoder currently exposes a single default frame
/// mode, so only `None` and `Some(0)` are accepted.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn lz4_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    validate_level(level)?;
    let mut encoder = lz4_flex::frame::FrameEncoder::new(writer);
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "lz4 compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e.into(), "lz4 compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using LZ4 with full options.
///
/// Currently only `options.level` is validated; `options.jobs` is accepted but
/// ignored because the selected frame encoder is single-threaded.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn lz4_compress_with_options<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
) -> GeeZipResult<u64> {
    lz4_compress_with_level(reader, writer, options.level)
}

/// Compress data from `reader` into `writer` using LZ4 with the default frame settings.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn lz4_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    lz4_compress_with_level(reader, writer, None)
}

/// Decompress an LZ4 frame from `reader` into `writer`.
///
/// Returns the number of bytes written to the output (decompressed size).
pub fn lz4_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = lz4_flex::frame::FrameDecoder::new(reader);
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "lz4 decompression failed"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn lz4_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of lz4 compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            lz4_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        assert_eq!(
            &compressed[..4],
            &[0x04, 0x22, 0x4D, 0x18],
            "lz4 frame magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = lz4_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn lz4_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            lz4_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce an lz4 frame"
        );
        assert_eq!(&compressed[..4], &[0x04, 0x22, 0x4D, 0x18]);

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = lz4_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn lz4_corrupted_data_fails() {
        let bad_data = b"this is not lz4 data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = lz4_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("lz4")
                || msg.contains("frame")
                || msg.contains("invalid")
                || msg.contains("io"),
            "expected lz4/io error, got: {err}"
        );
    }

    #[test]
    fn lz4_truncated_data_fails() {
        let original = vec![0xABu8; 128 * 1024];
        let mut source = Cursor::new(original.as_slice());
        let mut compressed = {
            let mut buf = Vec::new();
            lz4_compress(&mut source, &mut buf).unwrap();
            buf
        };
        compressed.truncate(compressed.len() / 2);

        let mut reader = Cursor::new(compressed.as_slice());
        let mut output = Vec::new();
        let err = lz4_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("lz4") || msg.contains("frame") || msg.contains("unexpected"),
            "expected truncated lz4 error, got: {err}"
        );
    }

    #[test]
    fn lz4_level_0_is_allowed() {
        let original = b"Hello, GeeZipX! LZ4 default-level test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            lz4_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        lz4_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn lz4_invalid_level_returns_error() {
        let mut source = Cursor::new(b"lz4 level bounds".as_slice());
        let err = lz4_compress_with_level(&mut source, Vec::new(), Some(1)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("use 0 or omit"), "unexpected message: {msg}");
    }
}
