//! Non-solid, single-volume, unencrypted EGG with seek-based block access.
//! Payloads stream through bounded output/CRC checks; metadata is cached once.
use super::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use unegg::archive::{CompressionMethod, EggArchive};

fn error(message: impl std::fmt::Display) -> GeeZipError {
    GeeZipError::format(format!("EGG: {message}"), ArchiveFormat::Egg)
}
pub struct EggReader {
    inner: EggArchive<File>,
}
impl EggReader {
    pub fn open(path: &Path) -> GeeZipResult<Self> {
        let inner = EggArchive::open(File::open(path)?).map_err(error)?;
        if inner.is_solid || inner.split_info.is_some() || inner.is_encrypted {
            return Err(error(
                "solid, split and encrypted archives are not supported",
            ));
        }
        Ok(Self { inner })
    }
}
struct CheckedWriter<'a> {
    writer: &'a mut dyn Write,
    remaining: u64,
    crc: crc32fast::Hasher,
}
impl Write for CheckedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("EGG output exceeds declared size"));
        }
        let n = self.writer.write(bytes)?;
        self.remaining -= n as u64;
        self.crc.update(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
impl ArchiveReader for EggReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Egg
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner
            .entries
            .iter()
            .map(|e| {
                let compressed_size = e
                    .blocks
                    .iter()
                    .try_fold(0u64, |sum, b| sum.checked_add(u64::from(b.compressed_size)))
                    .ok_or_else(|| error("compressed size overflow"))?;
                Ok(Entry {
                    path: e.file_name.clone(),
                    size: e.uncompressed_size,
                    compressed_size,
                    crc32: None,
                    modified: e
                        .file_time
                        .and_then(|v| (v / 10_000_000).checked_sub(11_644_473_600)),
                    is_dir: e.is_directory(),
                })
            })
            .collect()
    }
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let e = self
            .inner
            .entries
            .iter()
            .find(|e| e.file_name == entry.path)
            .ok_or_else(|| GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })?;
        if e.is_directory() {
            return Ok(0);
        }
        let expected = e
            .blocks
            .iter()
            .try_fold(0u64, |sum, b| {
                sum.checked_add(u64::from(b.uncompressed_size))
            })
            .ok_or_else(|| error("size overflow"))?;
        if expected != e.uncompressed_size {
            return Err(error("file/block size mismatch"));
        }
        for block in &e.blocks {
            self.inner.reader.seek(SeekFrom::Start(block.data_pos))?;
            let input = (&mut self.inner.reader).take(u64::from(block.compressed_size));
            let mut output = CheckedWriter {
                writer,
                remaining: u64::from(block.uncompressed_size),
                crc: crc32fast::Hasher::new(),
            };
            match block.compression_method {
                CompressionMethod::Store => {
                    std::io::copy(&mut input.take(u64::MAX), &mut output)?;
                }
                CompressionMethod::Deflate => {
                    std::io::copy(&mut flate2::read::DeflateDecoder::new(input), &mut output)?;
                }
                CompressionMethod::Bzip2 => {
                    std::io::copy(&mut bzip2::read::BzDecoder::new(input), &mut output)?;
                }
                CompressionMethod::Lzma => {
                    if block.compressed_size < 9 {
                        return Err(error("short LZMA header"));
                    }
                    let mut input = input;
                    let mut egg_header = [0; 9];
                    input.read_exact(&mut egg_header)?;
                    let mut header = [0; 13];
                    header[..5].copy_from_slice(&egg_header[4..]);
                    header[5..].fill(0xff);
                    let stream =
                        xz2::stream::Stream::new_lzma_decoder(256 * 1024 * 1024).map_err(error)?;
                    std::io::copy(
                        &mut xz2::read::XzDecoder::new_stream(
                            Cursor::new(header).chain(input),
                            stream,
                        ),
                        &mut output,
                    )?;
                }
                method => return Err(error(format!("unsupported method {method}"))),
            }
            if output.remaining != 0 || output.crc.finalize() != block.crc32 {
                return Err(error("block size or CRC mismatch"));
            }
        }
        Ok(expected)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn archive(method: u8, data: &[u8], packed: &[u8]) -> Vec<u8> {
        let mut out = b"EGGA".to_vec();
        out.extend(256u16.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(0x08E28222u32.to_le_bytes());
        out.extend(0x0A8590E3u32.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend((data.len() as u64).to_le_bytes());
        out.extend(0x0A8591ACu32.to_le_bytes());
        out.push(0);
        out.extend(8u16.to_le_bytes());
        out.extend(b"test.txt");
        out.extend(0x08E28222u32.to_le_bytes());
        out.extend(0x02B50C13u32.to_le_bytes());
        out.extend([method, 0]);
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((packed.len() as u32).to_le_bytes());
        out.extend(crc32fast::hash(data).to_le_bytes());
        out.extend(0x08E28222u32.to_le_bytes());
        out.extend(packed);
        out.extend(0x08E28222u32.to_le_bytes());
        out
    }
    #[test]
    fn store_deflate_bzip_lzma_and_crc() {
        let data = b"EGG payload".repeat(1024);
        let mut deflate =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        deflate.write_all(&data).unwrap();
        let mut bzip = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        bzip.write_all(&data).unwrap();
        let mut lzma = Vec::new();
        crate::archive::xz::lzma_compress(&mut data.as_slice(), &mut lzma).unwrap();
        let mut egg_lzma = vec![0; 4];
        egg_lzma.extend(&lzma[..5]);
        egg_lzma.extend(&lzma[13..]);
        for (method, packed) in [
            (0, data.clone()),
            (1, deflate.finish().unwrap()),
            (2, bzip.finish().unwrap()),
            (4, egg_lzma),
        ] {
            let mut bytes = archive(method, &data, &packed);
            let temp = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(temp.path(), &bytes).unwrap();
            let mut reader = EggReader::open(temp.path()).unwrap();
            let entry = reader.entries().unwrap().remove(0);
            let mut output = Vec::new();
            reader.extract(&entry, &mut output).unwrap();
            assert_eq!(output, data);
            // The block CRC is immediately before its end marker and payload.
            let pos = bytes.len() - packed.len() - 12;
            bytes[pos] ^= 1;
            std::fs::write(temp.path(), bytes).unwrap();
            let mut reader = EggReader::open(temp.path()).unwrap();
            assert!(reader.extract(&entry, &mut std::io::sink()).is_err());
        }
    }
}
