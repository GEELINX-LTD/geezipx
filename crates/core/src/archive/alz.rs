//! Seekable ALZ reader. Store and DEFLATE payloads stream through bounded buffers.
use crate::{
    archive::{ArchiveReader, Entry},
    detect::ArchiveFormat,
    error::{GeeZipError, GeeZipResult},
};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use unalz::{
    archive::{AlzArchive, CompressionMethod},
    crypto::ZipCrypto,
};

pub struct AlzReader {
    reader: std::fs::File,
    entries: Vec<unalz::archive::AlzFileEntry>,
    index: HashMap<String, usize>,
    password: Option<String>,
}
fn error(e: impl std::fmt::Display) -> GeeZipError {
    GeeZipError::format(format!("ALZ: {e}"), ArchiveFormat::Alz)
}
impl AlzReader {
    pub fn open(path: &Path, password: Option<String>) -> GeeZipResult<Self> {
        let path = path
            .to_str()
            .ok_or_else(|| error("archive path must be UTF-8"))?;
        let archive = AlzArchive::open(path).map_err(error)?;
        let reader = std::fs::File::open(path).map_err(error)?;
        if archive.reader.total_size() != reader.metadata().map_err(error)?.len() {
            return Err(error("split ALZ is not yet supported"));
        }
        if archive.truncated {
            return Err(error("truncated archive index or missing volume"));
        }
        let mut index = HashMap::new();
        for (i, e) in archive.entries.iter().enumerate() {
            index.entry(e.file_name.replace('\\', "/")).or_insert(i);
        }
        Ok(Self {
            reader,
            entries: archive.entries,
            index,
            password,
        })
    }
}
impl ArchiveReader for AlzReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Alz
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        Ok(self
            .entries
            .iter()
            .map(|e| Entry {
                path: e.file_name.replace('\\', "/"),
                size: e.uncompressed_size,
                compressed_size: e.compressed_size,
                crc32: Some(e.file_crc),
                modified: None,
                is_dir: e.is_directory(),
            })
            .collect())
    }
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let i = *self
            .index
            .get(&entry.path)
            .ok_or_else(|| GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })?;
        let e = &self.entries[i];
        if e.is_directory() {
            return Ok(0);
        }
        if !matches!(
            e.compression_method,
            CompressionMethod::Store | CompressionMethod::Deflate
        ) {
            return Err(error(
                "modified bzip2/method 3 not yet supported with bounded streaming memory",
            ));
        }
        let mut crypto = if e.is_encrypted() {
            let pwd = self
                .password
                .as_deref()
                .ok_or_else(|| error("password required"))?;
            let header = e
                .enc_check
                .as_ref()
                .ok_or_else(|| error("missing encryption header"))?;
            let mut c = ZipCrypto::new(&unalz::encoding::password_to_cp949(pwd));
            if !c.check_header(
                header,
                e.file_crc,
                e.file_time_date,
                e.has_data_descriptor(),
            ) {
                return Err(error("incorrect password"));
            }
            let mut c = ZipCrypto::new(&unalz::encoding::password_to_cp949(pwd));
            let mut h = *header;
            c.decrypt(&mut h);
            Some(c)
        } else {
            None
        };
        self.reader
            .seek(SeekFrom::Start(e.data_pos))
            .map_err(error)?;
        let mut input = (&mut self.reader).take(e.compressed_size);
        struct Count<'a> {
            inner: &'a mut dyn Write,
            bytes: u64,
        }
        impl Write for Count<'_> {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                let n = self.inner.write(b)?;
                self.bytes += n as u64;
                Ok(n)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.inner.flush()
            }
        }
        let mut output = Count {
            inner: writer,
            bytes: 0,
        };
        let crc = match e.compression_method {
            CompressionMethod::Store => unalz::decompress::raw::extract_raw(
                &mut input,
                &mut output,
                e.compressed_size,
                e.uncompressed_size,
                crypto.as_mut(),
            ),
            CompressionMethod::Deflate => unalz::decompress::deflate::extract_deflate(
                &mut input,
                &mut output,
                e.compressed_size,
                e.uncompressed_size,
                crypto.as_mut(),
            ),
            _ => unreachable!(),
        }
        .map_err(error)?;
        if crc != e.file_crc || output.bytes != e.uncompressed_size {
            return Err(error("CRC or uncompressed size mismatch"));
        }
        Ok(output.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(method: u8, data: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        if method == 2 {
            let mut z =
                flate2::write::DeflateEncoder::new(&mut payload, flate2::Compression::default());
            z.write_all(data).unwrap();
            z.finish().unwrap();
        } else {
            payload.extend_from_slice(data);
        }
        let mut b = b"ALZ\x01\x0a\x00\x00\x00BLZ\x01".to_vec();
        b.extend_from_slice(&[5, 0, 0x20, 0, 0, 0, 0, 0x40, 0, method, 0]);
        b.extend_from_slice(&crc32fast::hash(data).to_le_bytes());
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"a.txt");
        b.extend_from_slice(&payload);
        b.extend_from_slice(b"CLZ\x01\0\0\0\0CLZ\x02\0\0\0\0");
        b
    }
    #[test]
    fn stored_and_deflate_validate_crc() {
        for method in [0, 2] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("test.alz");
            let data = b"ALZ streaming ".repeat(10000);
            std::fs::write(&path, fixture(method, &data)).unwrap();
            let mut r = AlzReader::open(&path, None).unwrap();
            let e = r.entries().unwrap().remove(0);
            let mut out = Vec::new();
            assert_eq!(r.extract(&e, &mut out).unwrap(), data.len() as u64);
            assert_eq!(out, data);
            let mut broken = fixture(method, &data);
            broken[23] ^= 1;
            std::fs::write(&path, broken).unwrap();
            let mut r = AlzReader::open(&path, None).unwrap();
            assert!(r.extract(&e, &mut Vec::new()).is_err());
        }
    }
}
