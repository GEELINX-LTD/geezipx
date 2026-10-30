//! Read Compound File Binary storages (MSI/OLE). Streams are exposed unchanged;
//! MSI database interpretation and recursive embedded-CAB expansion are separate.
use super::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct CompoundReader {
    inner: cfb::CompoundFile<File>,
}
impl CompoundReader {
    pub fn open(path: &Path) -> GeeZipResult<Self> {
        Ok(Self {
            inner: cfb::open(path).map_err(|e| GeeZipError::io(e, "opening compound storage"))?,
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
