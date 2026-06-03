//! Archive and compression stream integrity verification.
//!
//! Provides [`verify_archive_reader`] for archive-based formats (zip, tar,
//! tar.gz, tar.zst, tar.xz) and [`verify_single_stream`] for single-stream
//! compression formats (gzip, zstd, xz, lzma).
//!
//! # Design
//!
//! Verification does **not** decompress to disk.  Every entry's payload is
//! streamed to `std::io::sink()`; the only side effect is that the reader
//! sees the full byte stream and can validate format-specific integrity
//! checks (e.g. ZIP CRC-32, compressed-stream checksum).
//!
//! TAR-based formats do not have per-entry CRC-32 values, so
//! [`TestReport::crc32_verified`] will be `false` for those.

use std::path::Path;

use crate::archive::ArchiveReader;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// Result of an integrity verification.
#[derive(Debug, Clone)]
pub struct TestReport {
    /// The detected format.
    pub format: ArchiveFormat,
    /// Number of entries processed (including directories).  For single-stream
    /// formats this is always 1.
    pub entry_count: u64,
    /// Uncompressed bytes read.
    pub bytes_read: u64,
    /// Whether per-entry CRC-32 checksums were validated.
    ///
    /// Currently `true` only for ZIP archives (the `zip` crate validates CRC-32
    /// when a file entry is read to completion).  TAR-based formats and
    /// single-stream compressors do not expose per-entry CRC-32 verification
    /// through GeeZipX's abstraction layer.
    pub crc32_verified: bool,
}

/// Verify the integrity of an archive by listing and reading every entry to
/// completion.
///
/// Each non-directory entry is extracted to `std::io::sink()`.  For ZIP
/// archives this triggers internal CRC-32 validation by the `zip` crate; a
/// mismatch produces a `GeeZipError::Format` error.
pub fn verify_archive_reader(reader: &mut dyn ArchiveReader) -> GeeZipResult<TestReport> {
    let format = reader.format();
    let entries = reader.entries()?;
    let entry_count = entries.len() as u64;
    let mut bytes_read = 0u64;

    // ZIP validates CRC-32 internally when reading a file entry to completion.
    let crc32_verified = format == ArchiveFormat::Zip;

    for entry in &entries {
        if entry.is_dir {
            continue;
        }
        let mut sink = std::io::sink();
        let n = reader.extract(entry, &mut sink)?;
        bytes_read += n;
    }

    Ok(TestReport {
        format,
        entry_count,
        bytes_read,
        crc32_verified,
    })
}

/// Verify the integrity of a single-stream compressed file (gzip, zstd, xz,
/// lzma).
///
/// Opens the file, creates the appropriate decoder, and copies the entire
/// decompressed stream to `std::io::sink()`.  Returns an error if the
/// compressed data is truncated or corrupted.
pub fn verify_single_stream(path: &Path, format: ArchiveFormat) -> GeeZipResult<TestReport> {
    let file = std::fs::File::open(path).map_err(|e| GeeZipError::io(e, "opening archive"))?;

    let bytes_read: u64 = match format {
        ArchiveFormat::Gzip => {
            let mut decoder = flate2::read::GzDecoder::new(file);
            std::io::copy(&mut decoder, &mut std::io::sink())
                .map_err(|e| GeeZipError::io(e, "gzip verification failed"))?
        }
        ArchiveFormat::Zstd => {
            let mut decoder = zstd::stream::Decoder::new(file)
                .map_err(|e| GeeZipError::io(e, "zstd decoder init failed"))?;
            std::io::copy(&mut decoder, &mut std::io::sink())
                .map_err(|e| GeeZipError::io(e, "zstd verification failed"))?
        }
        ArchiveFormat::Xz => {
            let mut decoder = xz2::read::XzDecoder::new_multi_decoder(file);
            std::io::copy(&mut decoder, &mut std::io::sink())
                .map_err(|e| GeeZipError::io(e, "xz verification failed"))?
        }
        ArchiveFormat::Lzma => {
            let stream = xz2::stream::Stream::new_lzma_decoder(u64::MAX)
                .map_err(|e| GeeZipError::io(e.into(), "lzma decoder init failed"))?;
            let mut decoder = xz2::read::XzDecoder::new_stream(file, stream);
            std::io::copy(&mut decoder, &mut std::io::sink())
                .map_err(|e| GeeZipError::io(e, "lzma verification failed"))?
        }
        _ => {
            return Err(GeeZipError::format(
                format!("verification not supported for {format}"),
                format,
            ));
        }
    };

    Ok(TestReport {
        format,
        entry_count: 1,
        bytes_read,
        crc32_verified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal valid ZIP archive from entries in memory.
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip_writer = zip::ZipWriter::new(&mut buf);
            let options = zip::write::SimpleFileOptions::default();
            for (name, data) in entries {
                zip_writer
                    .start_file(*name, options)
                    .expect("start_file should work");
                zip_writer.write_all(data).expect("write should work");
            }
            zip_writer.finish().expect("finish should work");
        }
        buf.into_inner()
    }

    #[test]
    fn verify_reader_valid_zip() {
        let buf = build_zip(&[("hello.txt", b"hello world")]);
        let cursor = std::io::Cursor::new(buf);
        let mut reader =
            crate::archive::zip::ZipReader::new(cursor).expect("zip reader should be created");
        let report = verify_archive_reader(&mut reader).expect("verification should pass");
        assert_eq!(report.format, ArchiveFormat::Zip);
        assert_eq!(report.entry_count, 1);
        assert!(report.bytes_read > 0);
        assert!(report.crc32_verified);
    }

    #[test]
    fn verify_reader_corrupted_zip_fails() {
        // Corrupted zip data should fail to open reader.
        let buf = b"PK\x03\x04truncated garbage";
        let cursor = std::io::Cursor::new(buf.to_vec());
        let result = crate::archive::zip::ZipReader::new(cursor);
        assert!(result.is_err(), "corrupted zip should fail to open reader");
    }

    #[test]
    fn verify_single_stream_valid_gzip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.gz");

        let content = b"hello gzip verification";
        let mut out = std::fs::File::create(&path).unwrap();
        let mut encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        encoder.write_all(content).unwrap();
        encoder.finish().unwrap();
        drop(out);

        let report = verify_single_stream(&path, ArchiveFormat::Gzip)
            .expect("gzip verification should pass");
        assert_eq!(report.format, ArchiveFormat::Gzip);
        assert_eq!(report.entry_count, 1);
        assert_eq!(report.bytes_read, content.len() as u64);
        assert!(!report.crc32_verified);
    }

    #[test]
    fn verify_single_stream_truncated_gzip_fails() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("truncated.gz");

        // Only the gzip header bytes, no body/footer.
        std::fs::write(&path, [0x1F, 0x8B]).unwrap();

        let result = verify_single_stream(&path, ArchiveFormat::Gzip);
        assert!(result.is_err(), "truncated gzip should fail verification");
    }
}
