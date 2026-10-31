//! Read Compound File Binary storages (MSI/OLE). Streams are exposed unchanged;
//! MSI database interpretation and recursive embedded-CAB expansion are separate.
use super::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

// cfb builds a u32 FAT entry for every physical sector, including sparse
// padding. Check that cost before the dependency allocates its metadata.
const MAX_FAT_BYTES: u64 = 64 * 1024 * 1024;

pub struct CompoundReader {
    inner: cfb::CompoundFile<File>,
}
impl CompoundReader {
    pub fn open(path: &Path) -> GeeZipResult<Self> {
        let mut file = File::open(path)?;
        let mut header = [0; 32];
        file.read_exact(&mut header)?;
        let sector_shift = u16::from_le_bytes([header[30], header[31]]);
        if !matches!(sector_shift, 9 | 12) {
            return Err(GeeZipError::format(
                "invalid CFB sector size",
                ArchiveFormat::Compound,
            ));
        }
        let sectors = file
            .metadata()?
            .len()
            .div_ceil(1u64 << sector_shift)
            .saturating_sub(1);
        if sectors > MAX_FAT_BYTES / 4 {
            return Err(GeeZipError::format(
                "CFB FAT metadata exceeds 64 MiB budget",
                ArchiveFormat::Compound,
            ));
        }
        file.rewind()?;
        Ok(Self {
            inner: cfb::CompoundFile::open(file)
                .map_err(|e| GeeZipError::io(e, "opening compound storage"))?,
        })
    }
}
impl ArchiveReader for CompoundReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Compound
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner
            .walk()
            .filter(|e| !e.is_root())
            .map(|e| {
                let path = e
                    .path()
                    .strip_prefix("/")
                    .map_err(|_| GeeZipError::format("invalid CFB path", ArchiveFormat::Compound))?
                    .to_string_lossy()
                    .into_owned();
                Ok(Entry {
                    path,
                    size: e.len(),
                    compressed_size: e.len(),
                    crc32: None,
                    modified: None,
                    is_dir: e.is_storage(),
                })
            })
            .collect()
    }
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        if entry.is_dir {
            return Ok(0);
        }
        let mut stream = self.inner.open_stream(Path::new("/").join(&entry.path))?;
        let expected = stream.len();
        let copied = std::io::copy(&mut stream, writer)?;
        if copied != expected {
            return Err(GeeZipError::format(
                "truncated CFB stream",
                ArchiveFormat::Compound,
            ));
        }
        Ok(copied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)] // set_len creates sparse extents here; Windows needs FSCTL_SET_SPARSE.
    fn sparse_containers_exceeding_fat_budget_are_rejected() {
        for (version, shift) in [(cfb::Version::V3, 9), (cfb::Version::V4, 12)] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let mut compound =
                cfb::CompoundFile::create_with_version(version, file.reopen().unwrap()).unwrap();
            compound.flush().unwrap();
            drop(compound);
            // No payload allocation: opening must reject before the dependency
            // fills its in-memory FAT up to the sparse file's sector count.
            let sectors = MAX_FAT_BYTES / 4 + 1;
            file.as_file().set_len((sectors + 1) << shift).unwrap();
            let error = CompoundReader::open(file.path())
                .err()
                .expect("FAT budget must precede allocation");
            assert!(error.to_string().contains("budget"), "{error}");
        }
    }

    #[test]
    fn nested_streams_and_ministream_extract() {
        let file = tempfile::NamedTempFile::new().unwrap();
        {
            let mut compound = cfb::create(file.path()).unwrap();
            compound.create_storage("/nested").unwrap();
            compound
                .create_stream("/nested/small")
                .unwrap()
                .write_all(b"small")
                .unwrap();
            compound
                .create_stream("/large")
                .unwrap()
                .write_all(&vec![7; 20000])
                .unwrap();
            compound.flush().unwrap();
        }
        let mut reader = CompoundReader::open(file.path()).unwrap();
        let entries = reader.entries().unwrap();
        assert_eq!(entries.len(), 3);
        for entry in entries.iter().filter(|e| !e.is_dir) {
            let mut data = Vec::new();
            assert_eq!(reader.extract(entry, &mut data).unwrap(), entry.size);
            assert_eq!(
                data,
                if entry.path == "large" {
                    vec![7; 20000]
                } else {
                    b"small".to_vec()
                }
            );
        }
    }
}
