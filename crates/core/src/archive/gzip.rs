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

/// Compress data from `reader` into `writer` using gzip.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn gzip_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    let mut encoder = flate2::write::GzEncoder::new(writer, flate2::Compression::default());
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "gzip compression failed"))?;
    encoder
        .try_finish()
        .map_err(|e| GeeZipError::io(e, "gzip compression finalisation failed"))?;
    Ok(bytes)
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
}
