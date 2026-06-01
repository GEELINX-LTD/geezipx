//! Single-stream gzip compression and decompression helpers.
//!
//! These functions work on a single byte stream — they do **not**
//! implement [`ArchiveReader`] or [`ArchiveWriter`] because gzip is a
//! compression format, not an archive container.  Use [`TarGzReader`] /
//! [`TarGzWriter`] for tar.gz archives.
//!
//! [`ArchiveReader`]: super::ArchiveReader
//! [`ArchiveWriter`]: super::ArchiveWriter
//! [`TarGzReader`]: super::targz::TarGzReader
//! [`TarGzWriter`]: super::targz::TarGzWriter

use std::io::{Read, Write};

use crate::error::{GeeZipError, GeeZipResult};

/// Convert an optional compression level (0-9) to a `flate2::Compression`.
///
/// `None` maps to the default level (6).
fn level_to_compression(level: Option<u32>) -> flate2::Compression {
    match level {
        None => flate2::Compression::default(),
        Some(l) => flate2::Compression::new(l),
    }
}

/// Compress data from `reader` into `writer` using gzip at the given level.
///
/// `level` controls the gzip compression strength:
/// - `None`: use the default level (6).
/// - `Some(0)`: no compression (store only).
/// - `Some(1)`: fastest compression.
/// - `Some(6)`: default (good balance).
/// - `Some(9)`: best compression ratio (slowest).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn gzip_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    let compression = level_to_compression(level);
    let mut encoder = flate2::write::GzEncoder::new(writer, compression);
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "gzip compression failed"))?;
    encoder
        .try_finish()
        .map_err(|e| GeeZipError::io(e, "gzip compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using gzip with the default level.
///
/// This is a convenience wrapper around [`gzip_compress_with_level`] with
/// `level: None` (default compression).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn gzip_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    gzip_compress_with_level(reader, writer, None)
}

/// Decompress gzip-compressed data from `reader` into `writer`.
///
/// Returns the number of bytes written to the output (decompressed size).
pub fn gzip_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = flate2::read::GzDecoder::new(reader);
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "gzip decompression failed"))?;
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
    fn gzip_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of gzip compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            gzip_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        // compressed should be smaller for repetitive data, but at minimum
        // it must contain gzip magic.
        assert_eq!(compressed[..2], [0x1F, 0x8B], "gzip magic expected");

        // Decompress
        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = gzip_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn gzip_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            gzip_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce gzip stream"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = gzip_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn gzip_corrupted_data_fails() {
        let bad_data = b"this is not gzip data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = gzip_decompress(&mut reader, &mut output).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("gzip")
                || err.to_string().to_lowercase().contains("io")
                || err.to_string().to_lowercase().contains("invalid"),
            "expected gzip/io error, got: {err}"
        );
    }

    #[test]
    fn gzip_large_data() {
        // 1 MB of repeating data
        let original = vec![0xABu8; 1_048_576];
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            gzip_compress(&mut source, &mut buf).unwrap();
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
        let bytes = gzip_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn gzip_with_level_9() {
        let original = b"Hello, GeeZipX! Level 9 compression test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            gzip_compress_with_level(&mut source, &mut buf, Some(9)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(compressed[..2], [0x1F, 0x8B], "gzip magic expected");

        // Decompress and verify
        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        gzip_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn gzip_with_level_0_store() {
        let original = b"Hello, GeeZipX! Level 0 (store) test.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            gzip_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(compressed[..2], [0x1F, 0x8B], "gzip magic expected");

        // Decompress and verify
        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        gzip_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn gzip_level_none_falls_back_to_default() {
        // Both gzip_compress and gzip_compress_with_level with None should
        // produce valid gzip output.
        let original = b"GeeZipX default level test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed_default = {
            let mut buf = Vec::new();
            gzip_compress(&mut source, &mut buf).unwrap();
            buf
        };

        source.set_position(0);
        let compressed_with_level = {
            let mut buf = Vec::new();
            gzip_compress_with_level(&mut source, &mut buf, None).unwrap();
            buf
        };

        assert!(!compressed_default.is_empty());
        assert!(!compressed_with_level.is_empty());

        // Both should decompress correctly
        let mut out1 = Vec::new();
        let mut reader1 = Cursor::new(compressed_default.as_slice());
        gzip_decompress(&mut reader1, &mut out1).unwrap();
        assert_eq!(out1, original);

        let mut out2 = Vec::new();
        let mut reader2 = Cursor::new(compressed_with_level.as_slice());
        gzip_decompress(&mut reader2, &mut out2).unwrap();
        assert_eq!(out2, original);
    }
}
