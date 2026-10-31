//! Non-solid, single-volume, unencrypted EGG with seek-based block access.
//! Payloads stream through bounded output/CRC checks; metadata is cached once.
use super::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use unegg::archive::{CompressionMethod, EggFileEntry};

mod index;

fn error(message: impl std::fmt::Display) -> GeeZipError {
    GeeZipError::format(format!("EGG: {message}"), ArchiveFormat::Egg)
}
pub struct EggReader {
    reader: File,
    entries: Vec<EggFileEntry>,
    index: HashMap<String, usize>,
}
impl EggReader {
    pub fn open(path: &Path) -> GeeZipResult<Self> {
        let mut reader = File::open(path)?;
        let entries = index::parse(&mut reader)?;
        let mut index = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            if index.insert(entry.file_name.clone(), i).is_some() {
                return Err(error("duplicate filename"));
            }
        }
        Ok(Self {
            reader,
            entries,
            index,
        })
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
        self.entries
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
        let &i = self
            .index
            .get(&entry.path)
            .ok_or_else(|| GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })?;
        let e = &self.entries[i];
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
            self.reader.seek(SeekFrom::Start(block.data_pos))?;
            let mut input = (&mut self.reader).take(u64::from(block.compressed_size));
            let mut output = CheckedWriter {
                writer,
                remaining: u64::from(block.uncompressed_size),
                crc: crc32fast::Hasher::new(),
            };
            match block.compression_method {
                CompressionMethod::Store => {
                    std::io::copy(&mut input, &mut output)?;
                }
                CompressionMethod::Deflate => {
                    deflate_exact(&mut input, &mut output, u64::from(block.compressed_size))?;
                }
                CompressionMethod::Bzip2 => {
                    let mut decoder = bzip2::read::BzDecoder::new(&mut input);
                    std::io::copy(&mut decoder, &mut output)?;
                    if decoder.total_in() != u64::from(block.compressed_size) {
                        return Err(error("trailing data in BZip2 block"));
                    }
                }
                CompressionMethod::Lzma => {
                    if block.compressed_size < 9 {
                        return Err(error("short LZMA header"));
                    }
                    let mut egg_header = [0; 9];
                    input.read_exact(&mut egg_header)?;
                    let mut header = [0; 13];
                    header[..5].copy_from_slice(&egg_header[4..]);
                    header[5..].copy_from_slice(&u64::from(block.uncompressed_size).to_le_bytes());
                    let stream =
                        xz2::stream::Stream::new_lzma_decoder(256 * 1024 * 1024).map_err(error)?;
                    let mut decoder = xz2::read::XzDecoder::new_stream(
                        Cursor::new(header).chain(&mut input),
                        stream,
                    );
                    std::io::copy(&mut decoder, &mut output)?;
                    if decoder.total_in() != u64::from(block.compressed_size) + 4 {
                        return Err(error("trailing data in LZMA block"));
                    }
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
fn deflate_exact(
    input: &mut dyn Read,
    output: &mut dyn Write,
    compressed_size: u64,
) -> GeeZipResult<()> {
    use flate2::{Decompress, FlushDecompress, Status};
    let mut decoder = Decompress::new(false);
    let mut packed = [0; 32 * 1024];
    let mut unpacked = [0; 32 * 1024];
    let mut buffered = 0;
    let mut offset = 0;
    loop {
        if offset == buffered {
            buffered = input.read(&mut packed)?;
            offset = 0;
        }
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(
                &packed[offset..buffered],
                &mut unpacked,
                FlushDecompress::None,
            )
            .map_err(error)?;
        let consumed = (decoder.total_in() - before_in) as usize;
        let produced = (decoder.total_out() - before_out) as usize;
        offset += consumed;
        output.write_all(&unpacked[..produced])?;
        if status == Status::StreamEnd {
            if decoder.total_in() != compressed_size {
                return Err(error("trailing data in Deflate block"));
            }
            return Ok(());
        }
        if consumed == 0 && produced == 0 {
            return Err(error("incomplete Deflate block"));
        }
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
    fn real_lzma_posix_and_blockless_empty_samples() {
        for (bytes, expected) in [
            (
                include_bytes!("../../tests/fixtures/egg/lzma5.egg").as_slice(),
                vec![
                    (0, 0),
                    (95, 0x6a94d1bc),
                    (2060, 0xb54ccab2),
                    (4096, 0x1bb92dfc),
                    (3150, 0x7a553a9d),
                ],
            ),
            (
                include_bytes!("../../tests/fixtures/egg/posix.egg").as_slice(),
                vec![(54, 0x96e10d03), (25, 0x6c7b95e7), (19, 0xb3010a13)],
            ),
            (
                include_bytes!("../../tests/fixtures/egg/empty_noblock.egg").as_slice(),
                vec![(0, 0)],
            ),
        ] {
            let temp = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(temp.path(), bytes).unwrap();
            let mut reader = EggReader::open(temp.path()).unwrap();
            let entries = reader.entries().unwrap();
            assert_eq!(entries.len(), expected.len());
            for (entry, &(size, crc)) in entries.iter().zip(&expected) {
                let mut output = Vec::new();
                assert_eq!(reader.extract(entry, &mut output).unwrap(), size);
                assert_eq!(output.len() as u64, size);
                assert_eq!(crc32fast::hash(&output), crc);
            }
        }
    }

    #[test]
    fn reject_nonfinal_deflate_trailing_data_and_truncated_index() {
        let data = b"EGG strict boundaries";
        let mut packed = vec![0];
        packed.extend((data.len() as u16).to_le_bytes());
        packed.extend((!(data.len() as u16)).to_le_bytes());
        packed.extend(data);
        for trailing in [false, true] {
            if trailing {
                packed[0] = 1;
                packed.extend(b"junk");
            }
            let temp = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(temp.path(), archive(1, data, &packed)).unwrap();
            let mut reader = EggReader::open(temp.path()).unwrap();
            let entry = reader.entries().unwrap().remove(0);
            assert!(reader.extract(&entry, &mut std::io::sink()).is_err());
        }
        let valid = archive(0, data, data);
        let temp = tempfile::NamedTempFile::new().unwrap();
        for length in 0..valid.len() {
            std::fs::write(temp.path(), &valid[..length]).unwrap();
            assert!(EggReader::open(temp.path()).is_err(), "accepted {length}");
        }
        // Filename extra-field claims 4 GiB from a tiny archive.
        let mut oversized = valid;
        oversized[38] = 1;
        oversized.splice(39..41, u32::MAX.to_le_bytes());
        std::fs::write(temp.path(), oversized).unwrap();
        assert!(EggReader::open(temp.path()).is_err());
    }

    #[test]
    fn relative_names_require_existing_acyclic_unique_ids() {
        let mut bytes = archive(0, b"x", b"x");
        bytes[38] = 0x10; // relative path, parent ID precedes name
        bytes[39..41].copy_from_slice(&12u16.to_le_bytes());
        bytes.splice(41..41, 99u32.to_le_bytes());
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), &bytes).unwrap();
        assert!(EggReader::open(temp.path()).is_err()); // missing parent
        bytes[41..45].copy_from_slice(&1u32.to_le_bytes());
        std::fs::write(temp.path(), &bytes).unwrap();
        assert!(EggReader::open(temp.path()).is_err()); // self-cycle

        let normal = archive(0, b"x", b"x");
        let mut duplicate = normal[..normal.len() - 4].to_vec();
        duplicate.extend_from_slice(&normal[18..]);
        std::fs::write(temp.path(), &duplicate).unwrap();
        assert!(EggReader::open(temp.path()).is_err()); // duplicate ID
    }

    #[test]
    fn store_deflate_bzip_lzma_and_crc() {
        let data = b"EGG payload".repeat(1024);
        let mut deflate =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        deflate.write_all(&data).unwrap();
        let mut bzip = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        bzip.write_all(&data).unwrap();
        let options = lzma_rust2::LzmaOptions::with_preset(6);
        let mut encoder =
            lzma_rust2::LzmaWriter::new_no_header(Vec::new(), &options, false).unwrap();
        encoder.write_all(&data).unwrap();
        let lzma = encoder.finish().unwrap();
        let mut egg_lzma = vec![0; 4];
        egg_lzma.push(0x5d);
        egg_lzma.extend(options.dict_size.to_le_bytes());
        egg_lzma.extend(lzma);
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
