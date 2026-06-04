//! Single-stream Zstandard compression and decompression helpers.
//!
//! These functions work on a single byte stream — they do **not**
//! implement [`ArchiveReader`] or [`ArchiveWriter`] because zstd is a
//! compression format, not an archive container.
//!
//! [`ArchiveReader`]: super::ArchiveReader
//! [`ArchiveWriter`]: super::ArchiveWriter

use std::io::{Read, Write};

use crate::config::CompressOptions;
use crate::error::{GeeZipError, GeeZipResult};

/// Compress data from `reader` into `writer` using Zstandard at the given level.
///
/// `level` controls the zstd compression strength:
/// - `None`: use the zstd default level (currently 3).
/// - `Some(0)`: also maps to zstd default level (0 means "default" in zstd API).
/// - `Some(1..=22)`: specific compression level (higher = better ratio, slower).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn zstd_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    let level_i32 = match level {
        None | Some(0) => 0, // 0 means "default" in the zstd crate
        Some(l) => l as i32,
    };
    let mut encoder = zstd::stream::write::Encoder::new(writer, level_i32)
        .map_err(|e| GeeZipError::io(e, "zstd compression init failed"))?;
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "zstd compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e, "zstd compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using Zstandard with full options.
///
/// Supports both compression level and multi-threaded encoding via
/// [`CompressOptions`].  When `options.effective_jobs() > 1` the underlying
/// zstd encoder is configured to use that many worker threads.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn zstd_compress_with_options<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
) -> GeeZipResult<u64> {
    let level_i32 = match options.level {
        None | Some(0) => 0,
        Some(l) => l as i32,
    };
    let workers = options.effective_jobs() as u32;
    let mut encoder = zstd::stream::write::Encoder::new(writer, level_i32)
        .map_err(|e| GeeZipError::io(e, "zstd compression init failed"))?;
    if workers > 1 {
        encoder
            .set_parameter(zstd::stream::raw::CParameter::NbWorkers(workers))
            .map_err(|e| GeeZipError::io(e, "zstd multithread init failed"))?;
    }
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "zstd compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e, "zstd compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using Zstandard with the default level.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn zstd_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    zstd_compress_with_level(reader, writer, None)
}

/// Decompress Zstandard-compressed data from `reader` into `writer`.
///
/// Returns the number of bytes written to the output (decompressed size).
pub fn zstd_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = zstd::stream::read::Decoder::new(reader)
        .map_err(|e| GeeZipError::io(e, "zstd decompression init failed"))?;
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "zstd decompression failed"))?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn zstd_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of zstd compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            zstd_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        // Zstandard magic: 0x28 0xB5 0x2F 0xFD
        assert_eq!(
            compressed[..4],
            [0x28, 0xB5, 0x2F, 0xFD],
            "zstd magic expected"
        );

        // Decompress
        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn zstd_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            zstd_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce zstd stream"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn zstd_corrupted_data_fails() {
        let bad_data = b"this is not zstd data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = zstd_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("zstd") || msg.contains("io") || msg.contains("invalid"),
            "expected zstd/io error, got: {err}"
        );
    }

    #[test]
    fn zstd_large_data() {
        // 1 MB of repeating data
        let original = vec![0xABu8; 1_048_576];
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            zstd_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            compressed.len() < original.len(),
            "compressed size ({}) should be less than original ({}) for repetitive data",
            compressed.len(),
            original.len()
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn zstd_level_22() {
        let original = b"Hello, GeeZipX! Level 22 (max) compression test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            zstd_compress_with_level(&mut source, &mut buf, Some(22)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(
            compressed[..4],
            [0x28, 0xB5, 0x2F, 0xFD],
            "zstd magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn zstd_level_1() {
        let original = b"Hello, GeeZipX! Fastest level test.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            zstd_compress_with_level(&mut source, &mut buf, Some(1)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(
            compressed[..4],
            [0x28, 0xB5, 0x2F, 0xFD],
            "zstd magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn zstd_level_0_falls_back_to_default() {
        let original = b"GeeZipX zstd level 0 = default test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed_level0 = {
            let mut buf = Vec::new();
            zstd_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        source.set_position(0);
        let compressed_default = {
            let mut buf = Vec::new();
            zstd_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(!compressed_level0.is_empty());
        assert!(!compressed_default.is_empty());

        // Both should decompress correctly
        let mut out1 = Vec::new();
        let mut reader1 = Cursor::new(compressed_level0.as_slice());
        zstd_decompress(&mut reader1, &mut out1).unwrap();
        assert_eq!(out1, original);

        let mut out2 = Vec::new();
        let mut reader2 = Cursor::new(compressed_default.as_slice());
        zstd_decompress(&mut reader2, &mut out2).unwrap();
        assert_eq!(out2, original);
    }

    #[test]
    fn zstd_truncated_stream_fails() {
        // Valid zstd magic but truncated body.
        let truncated = b"\x28\xB5\x2F\xFD\x00\x00\x00\x00";
        let mut reader = std::io::Cursor::new(truncated.as_slice());
        let mut output = Vec::new();

        let err = zstd_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("zstd") || msg.contains("io") || msg.contains("invalid"),
            "expected zstd/io error for truncated zstd stream, got: {err}"
        );
    }

    #[test]
    fn zstd_compress_with_options_roundtrip() {
        let original = b"GeeZipX zstd compress_with_options test.";
        let mut source = Cursor::new(original.as_slice());

        let options = crate::config::CompressOptions {
            level: Some(9),
            jobs: None,
            password: None,
        };
        let compressed = {
            let mut buf = Vec::new();
            zstd_compress_with_options(&mut source, &mut buf, options).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        assert_eq!(
            compressed[..4],
            [0x28, 0xB5, 0x2F, 0xFD],
            "zstd magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn zstd_compress_with_options_multi_threaded() {
        let original = b"GeeZipX multi-threaded zstd compression test.";
        let mut source = Cursor::new(original.as_slice());

        let options = crate::config::CompressOptions {
            level: None,
            jobs: Some(2),
            password: None,
        };
        let compressed = {
            let mut buf = Vec::new();
            zstd_compress_with_options(&mut source, &mut buf, options).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        assert_eq!(
            compressed[..4],
            [0x28, 0xB5, 0x2F, 0xFD],
            "zstd magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = zstd_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }
}
