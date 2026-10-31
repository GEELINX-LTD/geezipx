//! Read TAR wrapped in LZMA Alone or lzip (`.tlz`, `.tar.lzma`, `.tar.lz`).
//! Codec detection uses a bounded lookahead; entry payloads are never buffered.
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use super::{ArchiveReader, Entry, EntryVisitor};
use crate::{
    detect::ArchiveFormat,
    error::{GeeZipError, GeeZipResult},
};

/// Decode either TLZ convention, selecting lzip by its unambiguous magic.
pub fn decoder<'a, R: Read + 'a>(mut reader: R) -> GeeZipResult<Box<dyn Read + 'a>> {
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    let source = Cursor::new(prefix).chain(reader);
    if prefix == *b"LZIP" {
        Ok(Box::new(super::lz::LzipReader::new(source)))
    } else {
        Ok(Box::new(super::xz::lzma_decoder(source)?))
    }
}

pub struct TarLzReader<R: Read + Seek + Send> {
    inner: R,
}
impl<R: Read + Seek + Send> TarLzReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }
}
impl<R: Read + Seek + Send> ArchiveReader for TarLzReader<R> {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::TarLz
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        let mut entries = Vec::new();
        self.visit_entries(
            &mut |entry, _| {
                entries.push(entry.clone());
                Ok(true)
            },
            &|| false,
        )?;
        Ok(entries)
    }
    fn has_direct_visitor(&self) -> bool {
        true
    }

    fn visit_entries(
        &mut self,
        visitor: &mut EntryVisitor<'_>,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<()> {
        self.inner.seek(SeekFrom::Start(0))?;
        let stream = decoder(&mut self.inner)?;
        super::tar::visit_tar_archive(tar::Archive::new(stream), visitor, is_cancelled)
    }
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let mut extracted = None;
        self.visit_entries(
            &mut |candidate, data| {
                if candidate.path != entry.path {
                    return Ok(true);
                }
                extracted = Some(if candidate.is_dir {
                    0
                } else {
                    std::io::copy(data, writer)?
                });
                Ok(false)
            },
            &|| false,
        )?;
        extracted.ok_or_else(|| GeeZipError::EntryNotFound {
            name: entry.path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{tar::TarWriter, ArchiveWriter};
    use super::*;
    use std::{io::Cursor, path::Path};
    #[test]
    fn tlz_rejects_oversized_lzma_dictionary_before_output() {
        let mut header = vec![0x5d];
        header.extend((1u32 << 30).to_le_bytes());
        header.extend(u64::MAX.to_le_bytes());
        header.extend([0; 32]);
        let mut output = Vec::new();
        assert!(std::io::copy(&mut decoder(header.as_slice()).unwrap(), &mut output).is_err());
        assert!(output.is_empty());
    }

    #[test]
    fn both_tlz_conventions_and_corruption() {
        let data = b"streaming tar payload".repeat(10000);
        let mut tar = TarWriter::new(Vec::new());
        tar.add_entry_with_size(
            Path::new("dir/文件.txt"),
            &mut data.as_slice(),
            data.len() as u64,
        )
        .unwrap();
        let (_, tar) = tar.finalize().unwrap();
        for lzip in [false, true] {
            let mut compressed = Vec::new();
            if lzip {
                super::super::lz::lz_compress(&mut tar.as_slice(), &mut compressed).unwrap();
            } else {
                super::super::xz::lzma_compress(&mut tar.as_slice(), &mut compressed).unwrap();
            }
            struct ShortReads<'a>(&'a [u8]);
            impl Read for ShortReads<'_> {
                fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                    let n = buf.len().min(1);
                    self.0.read(&mut buf[..n])
                }
            }
            let mut unwrapped = Vec::new();
            decoder(ShortReads(&compressed))
                .unwrap()
                .read_to_end(&mut unwrapped)
                .unwrap();
            assert_eq!(unwrapped, tar);
            let mut reader = TarLzReader::new(Cursor::new(&compressed));
            let entries = reader.entries().unwrap();
            assert_eq!(entries.len(), 1);
            let mut output = Vec::new();
            reader.extract(&entries[0], &mut output).unwrap();
            assert_eq!(output, data);
            compressed.truncate(compressed.len() - 5);
            assert!(TarLzReader::new(Cursor::new(compressed)).entries().is_err());
        }
    }
}
