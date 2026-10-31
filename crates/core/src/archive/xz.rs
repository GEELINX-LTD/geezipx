//! Single-stream XZ and LZMA compression and decompression helpers.
//!
//! These functions work on a single byte stream — they do **not**
//! implement [`ArchiveReader`] or [`ArchiveWriter`] because xz and lzma are
//! compression formats, not archive containers.
//!
//! [`ArchiveReader`]: super::ArchiveReader
//! [`ArchiveWriter`]: super::ArchiveWriter

use std::io::{Read, Write};

use crate::config::CompressOptions;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// XZ helpers
// ---------------------------------------------------------------------------

/// Compress data from `reader` into `writer` using XZ at the given level.
///
/// `level` controls the XZ compression strength:
/// - `None`: use the default level (6).
/// - `Some(0)`: fastest preset (still compressed).
/// - `Some(1)`: fastest compression.
/// - `Some(6)`: default (good balance).
/// - `Some(9)`: best compression ratio (slowest).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn xz_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    let lvl = level.unwrap_or(6);
    let stream = xz2::stream::Stream::new_easy_encoder(lvl, xz2::stream::Check::Crc64)
        .map_err(|e| GeeZipError::io(e.into(), "xz encoder init failed"))?;
    let mut encoder = xz2::write::XzEncoder::new_stream(writer, stream);
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "xz compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e, "xz compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using XZ with full options.
///
/// Compress using liblzma's bounded block-parallel encoder when `jobs > 1`.
/// The default stays single-threaded; each worker owns a codec dictionary and
/// block buffers, so callers should choose jobs/level to fit their memory budget.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn xz_compress_with_options<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
) -> GeeZipResult<u64> {
    let stream = encoder_stream(&options)?;
    let mut encoder = xz2::write::XzEncoder::new_stream(writer, stream);
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "xz compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e, "xz compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using XZ with the default level.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn xz_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    xz_compress_with_level(reader, writer, None)
}

/// Decompress XZ-compressed data from `reader` into `writer`.
///
/// Returns the number of bytes written to the output (decompressed size).
pub fn xz_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = xz2::read::XzDecoder::new_multi_decoder(reader);
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "xz decompression failed"))?;
    Ok(bytes)
}

pub(crate) fn encoder_stream(options: &CompressOptions) -> GeeZipResult<xz2::stream::Stream> {
    let level = options.level.unwrap_or(6);
    let jobs = options.effective_jobs().clamp(1, 256) as u32;
    let result = if jobs == 1 {
        xz2::stream::Stream::new_easy_encoder(level, xz2::stream::Check::Crc64)
    } else {
        xz2::stream::MtStreamBuilder::new()
            .threads(jobs)
            .preset(level)
            .check(xz2::stream::Check::Crc64)
            .encoder()
    };
    result.map_err(|e| GeeZipError::io(e.into(), "xz encoder init failed"))
}

// ---------------------------------------------------------------------------
// LZMA helpers
// ---------------------------------------------------------------------------

/// Compress data from `reader` into `writer` using LZMA at the given level.
///
/// `level` controls the LZMA compression strength:
/// - `None`: use the default level (6).
/// - `Some(0)`: fastest preset (still compressed).
/// - `Some(1..=9)`: specific compression level.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn lzma_compress_with_level<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    level: Option<u32>,
) -> GeeZipResult<u64> {
    let lvl = level.unwrap_or(6);
    let options = xz2::stream::LzmaOptions::new_preset(lvl)
        .map_err(|e| GeeZipError::io(e.into(), "lzma options init failed"))?;
    let stream = xz2::stream::Stream::new_lzma_encoder(&options)
        .map_err(|e| GeeZipError::io(e.into(), "lzma stream init failed"))?;
    let mut encoder = xz2::write::XzEncoder::new_stream(writer, stream);
    let bytes = std::io::copy(reader, &mut encoder)
        .map_err(|e| GeeZipError::io(e, "lzma compression failed"))?;
    encoder
        .finish()
        .map_err(|e| GeeZipError::io(e, "lzma compression finalisation failed"))?;
    Ok(bytes)
}

/// Compress data from `reader` into `writer` using LZMA with full options.
///
/// Currently only `options.level` is applied; `options.jobs` is accepted
/// but ignored (LZMA is inherently single-stream, no multithread encoder
/// support is expected).
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn lzma_compress_with_options<R: Read, W: Write>(
    reader: &mut R,
    writer: W,
    options: CompressOptions,
) -> GeeZipResult<u64> {
    lzma_compress_with_level(reader, writer, options.level)
}

/// Compress data from `reader` into `writer` using LZMA with the default level.
///
/// Returns the number of bytes read from the source (uncompressed size).
pub fn lzma_compress<R: Read, W: Write>(reader: &mut R, writer: W) -> GeeZipResult<u64> {
    lzma_compress_with_level(reader, writer, None)
}

/// Maximum decoder memory for untrusted LZMA Alone/Lzip streams (256 MiB).
pub(crate) const LZMA_MEMORY_LIMIT: u64 = 256 * 1024 * 1024;

pub(crate) fn lzma_decoder<R: Read>(reader: R) -> GeeZipResult<xz2::read::XzDecoder<R>> {
    let stream = xz2::stream::Stream::new_lzma_decoder(LZMA_MEMORY_LIMIT)
        .map_err(|e| GeeZipError::io(e.into(), "lzma decompression init failed"))?;
    Ok(xz2::read::XzDecoder::new_stream(reader, stream))
}

/// Decompress LZMA-compressed data with at most 256 MiB of decoder memory.
/// Returns the number of bytes written to the output.
pub fn lzma_decompress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> GeeZipResult<u64> {
    let mut decoder = lzma_decoder(reader)?;
    let bytes = std::io::copy(&mut decoder, writer)
        .map_err(|e| GeeZipError::io(e, "lzma decompression failed"))?;
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
    fn xz_and_lzma_reject_invalid_presets_without_panicking() {
        for level in [10, u32::MAX] {
            assert!(
                xz_compress_with_level(&mut &b"input"[..], std::io::sink(), Some(level)).is_err()
            );
            assert!(
                lzma_compress_with_level(&mut &b"input"[..], std::io::sink(), Some(level)).is_err()
            );
        }
    }

    // ---------------------------------------------------------------
    // XZ tests
    // ---------------------------------------------------------------

    #[test]
    fn xz_parallel_and_invalid_options() {
        let data = b"parallel blocks ".repeat(8192);
        let mut compressed = Vec::new();
        let options = CompressOptions::default()
            .with_jobs(Some(2))
            .with_level(Some(1));
        xz_compress_with_options(&mut data.as_slice(), &mut compressed, options).unwrap();
        let mut output = Vec::new();
        xz_decompress(&mut compressed.as_slice(), &mut output).unwrap();
        assert_eq!(output, data);
        assert!(xz_compress_with_options(
            &mut &b"x"[..],
            &mut Vec::new(),
            CompressOptions::default().with_level(Some(10))
        )
        .is_err());
    }

    #[test]
    fn xz_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of xz compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            xz_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );
        // XZ magic: 0xFD 0x37 0x7A 0x58 0x5A 0x00
        assert_eq!(
            &compressed[..6],
            &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00],
            "xz magic expected"
        );

        // Decompress
        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = xz_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn xz_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            xz_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce xz stream"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = xz_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn xz_corrupted_data_fails() {
        let bad_data = b"this is not xz data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = xz_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("xz") || msg.contains("io") || msg.contains("invalid"),
            "expected xz/io error, got: {err}"
        );
    }

    #[test]
    fn xz_large_data() {
        // 1 MB of repeating data
        let original = vec![0xABu8; 1_048_576];
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            xz_compress(&mut source, &mut buf).unwrap();
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
        let bytes = xz_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn xz_with_level_9() {
        let original = b"Hello, GeeZipX! Level 9 xz compression test data.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            xz_compress_with_level(&mut source, &mut buf, Some(9)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(
            &compressed[..6],
            &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00],
            "xz magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        xz_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn xz_with_level_0() {
        let original = b"Hello, GeeZipX! Level 0 (store) xz test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            xz_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());
        assert_eq!(
            &compressed[..6],
            &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00],
            "xz magic expected"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        xz_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn xz_level_none_falls_back_to_default() {
        let original = b"GeeZipX default xz level test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed_default = {
            let mut buf = Vec::new();
            xz_compress(&mut source, &mut buf).unwrap();
            buf
        };

        source.set_position(0);
        let compressed_with_level = {
            let mut buf = Vec::new();
            xz_compress_with_level(&mut source, &mut buf, None).unwrap();
            buf
        };

        assert!(!compressed_default.is_empty());
        assert!(!compressed_with_level.is_empty());

        // Both should decompress correctly
        let mut out1 = Vec::new();
        let mut reader1 = Cursor::new(compressed_default.as_slice());
        xz_decompress(&mut reader1, &mut out1).unwrap();
        assert_eq!(out1, original);

        let mut out2 = Vec::new();
        let mut reader2 = Cursor::new(compressed_with_level.as_slice());
        xz_decompress(&mut reader2, &mut out2).unwrap();
        assert_eq!(out2, original);
    }

    #[test]
    fn xz_truncated_stream_fails() {
        // Valid XZ magic but truncated body.
        let truncated = b"\xFD\x37\x7A\x58\x5A\x00\x00\x00";
        let mut reader = std::io::Cursor::new(truncated.as_slice());
        let mut output = Vec::new();

        let err = xz_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("xz") || msg.contains("io") || msg.contains("invalid"),
            "expected xz/io error for truncated xz stream, got: {err}"
        );
    }

    // ---------------------------------------------------------------
    // LZMA tests
    // ---------------------------------------------------------------

    #[test]
    fn lzma_roundtrip() {
        let original = b"Hello, GeeZipX! This is a test of lzma compression.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            lzma_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "compressed output should not be empty"
        );

        // Decompress
        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = lzma_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn lzma_empty_data() {
        let mut source = Cursor::new(b"");
        let compressed = {
            let mut buf = Vec::new();
            lzma_compress(&mut source, &mut buf).unwrap();
            buf
        };

        assert!(
            !compressed.is_empty(),
            "empty data should still produce lzma stream"
        );

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = lzma_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, 0);
        assert!(decompressed.is_empty());
    }

    #[test]
    fn lzma_corrupted_data_fails() {
        let bad_data = b"this is not lzma data at all!";
        let mut reader = Cursor::new(bad_data.as_slice());
        let mut output = Vec::new();

        let err = lzma_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("lzma") || msg.contains("io") || msg.contains("invalid"),
            "expected lzma/io error, got: {err}"
        );
    }

    #[test]
    fn lzma_large_data() {
        let original = vec![0xCDu8; 1_048_576];
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            lzma_compress(&mut source, &mut buf).unwrap();
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
        let bytes = lzma_decompress(&mut compressed_reader, &mut decompressed).unwrap();

        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn lzma_with_level_9() {
        let original = b"Hello, GeeZipX! Level 9 lzma compression test data.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            lzma_compress_with_level(&mut source, &mut buf, Some(9)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        lzma_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn lzma_with_level_0() {
        let original = b"Hello, GeeZipX! Level 0 (store) lzma test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed = {
            let mut buf = Vec::new();
            lzma_compress_with_level(&mut source, &mut buf, Some(0)).unwrap();
            buf
        };

        assert!(!compressed.is_empty());

        let mut decompressed = Vec::new();
        let mut compressed_reader = Cursor::new(compressed.as_slice());
        lzma_decompress(&mut compressed_reader, &mut decompressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn lzma_level_none_falls_back_to_default() {
        let original = b"GeeZipX default lzma level test.";
        let mut source = Cursor::new(original.as_slice());

        let compressed_default = {
            let mut buf = Vec::new();
            lzma_compress(&mut source, &mut buf).unwrap();
            buf
        };

        source.set_position(0);
        let compressed_with_level = {
            let mut buf = Vec::new();
            lzma_compress_with_level(&mut source, &mut buf, None).unwrap();
            buf
        };

        assert!(!compressed_default.is_empty());
        assert!(!compressed_with_level.is_empty());

        let mut out1 = Vec::new();
        let mut reader1 = Cursor::new(compressed_default.as_slice());
        lzma_decompress(&mut reader1, &mut out1).unwrap();
        assert_eq!(out1, original);

        let mut out2 = Vec::new();
        let mut reader2 = Cursor::new(compressed_with_level.as_slice());
        lzma_decompress(&mut reader2, &mut out2).unwrap();
        assert_eq!(out2, original);
    }

    #[test]
    fn lzma_truncated_stream_fails() {
        let truncated = b"\x5D\x00\x00\x00\x00\x00\x00\x00";
        let mut reader = std::io::Cursor::new(truncated.as_slice());
        let mut output = Vec::new();

        let err = lzma_decompress(&mut reader, &mut output).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("lzma") || msg.contains("io") || msg.contains("invalid"),
            "expected lzma/io error for truncated lzma stream, got: {err}"
        );
    }
}
