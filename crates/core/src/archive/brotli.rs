//! Single-stream Brotli compression and decompression helpers.
//!
//! These functions operate on a single byte stream — they do **not** implement
//! [`ArchiveReader`] or [`ArchiveWriter`] because Brotli is a compression
//! format, not an archive container. Use [`TarBrReader`] / [`TarBrWriter`] for
//! `.tar.br` archives.
//!
//! [`ArchiveReader`]: super::ArchiveReader
//! [`ArchiveWriter`]: super::ArchiveWriter
//! [`TarBrReader`]: super::tarbr::TarBrReader
//! [`TarBrWriter`]: super::tarbr::TarBrWriter

use std::io::{Read, Write};

use brotli::enc::backward_references::BrotliEncoderParams;

use crate::config::CompressOptions;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

const BROTLI_BUFFER_SIZE: usize = 64 * 1024;

struct CountingReader<'a, R> {
    inner: &'a mut R,
    bytes_read: u64,
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n as u64;
        Ok(n)
    }
}

fn compression_params(level: Option<u32>) -> GeeZipResult<BrotliEncoderParams> {
    let mut params = BrotliEncoderParams::default();
    match level {
        None => {}
        Some(l @ 0..=11) => params.quality = l as i32,
        Some(l) => {
            return Err(GeeZipError::format(
                format!("brotli compression level must be 0..=11, got {l}"),
                ArchiveFormat::Brotli,
            ));
        }
    }
    Ok(params)
}

/// Compress data from `reader` into `writer` using Brotli at the given level.
///
/// `level` controls Brotli quality:
/// - `None`: use the crate default quality (11).
/// - `Some(0..=11)`: explicit Brotli quality.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn brotli_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    mut writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    let params = compression_params(level)?;
    let mut counting_reader = CountingReader {
        inner: reader,
        bytes_read: 0,
    };
    brotli::BrotliCompress(&mut counting_reader, &mut writer, &params)
        .map_err(|e| GeeZipError::io(e, "brotli compression failed"))?;
    Ok(counting_reader.bytes_read)
}

/// Compress data from `reader` into `writer` using Brotli with full options.
///
/// Currently only `options.level` is applied; `options.jobs` is accepted but
/// ignored because the selected Brotli encoder path is single-threaded.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn brotli_compress_with_options<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
) -> GeeZipResult<u64> {
    brotli_compress_with_level(reader, writer, options.level)
}

/// Compress data from `reader` into `writer` using Brotli with the default level.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn brotli_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    brotli_compress_with_level(reader, writer, None)
}

/// Decompress Brotli-compressed data from `reader` into `writer`.
///
/// Returns the number of bytes written to the output (decompressed size).
pub fn brotli_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = brotli::Decompressor::new(reader, BROTLI_BUFFER_SIZE);
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "brotli decompression failed"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn brotli_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of brotli compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            brotli_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = brotli_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn brotli_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            brotli_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce a valid brotli stream"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = brotli_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn brotli_corrupted_data_fails() {
        let bad_data = b"this is not brotli data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = brotli_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("brotli") || msg.contains("invalid") || msg.contains("io"),
            "expected brotli/io error, got: {err}"
        );
    }

    #[test]
    fn brotli_truncated_data_fails() {
        let mut source = Cursor::new(b"truncated brotli payload".as_slice());
        let mut compressed = {
            let mut buf = Vec::new();
            brotli_compress(&mut source, &mut buf).unwrap();
            buf
        };
        compressed.truncate(compressed.len().saturating_sub(2));

        let mut reader = Cursor::new(compressed.as_slice());
        let mut output = Vec::new();
        let err = brotli_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("brotli") || msg.contains("unexpected") || msg.contains("invalid"),
            "expected truncated brotli error, got: {err}"
        );
    }

    #[test]
    fn brotli_with_level_11() {
        let original = b"Hello, GeeZipX! Level 11 brotli compression test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            brotli_compress_with_level(&mut source, &mut buf, Some(11)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        brotli_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn brotli_with_level_0() {
        let original = b"Hello, GeeZipX! Level 0 brotli compression test data.";
        let mut source = Cursor::new(original.as_slice());
        let compressed = {
            let mut buf = Vec::new();
            brotli_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        brotli_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn brotli_invalid_level_returns_error() {
        let mut source = Cursor::new(b"brotli level bounds".as_slice());
        let err = brotli_compress_with_level(&mut source, Vec::new(), Some(12)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("0..=11"), "unexpected message: {msg}");
    }
}
