//! Optical filesystem extraction from cooked images and 2352-byte CD sectors.
//! Only Mode 1 and Mode 2 Form 1 data tracks are supported. Mixed/audio tracks,
//! CUE layouts and partitioned disk filesystems require separate adapters.
use super::{iso::IsoReader, ArchiveReader, Entry};
use crate::{detect::ArchiveFormat, GeeZipError, GeeZipResult};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct DiskImageReader {
    inner: IsoReader<Sectors<BufReader<File>>>,
    format: ArchiveFormat,
}
impl DiskImageReader {
    pub fn open(path: &Path, format: ArchiveFormat) -> GeeZipResult<Self> {
        let source = BufReader::with_capacity(64 * 1024, File::open(path)?);
        Ok(Self {
            inner: IsoReader::new(Sectors::new(source)?),
            format,
        })
    }
}
impl ArchiveReader for DiskImageReader {
    fn format(&self) -> ArchiveFormat {
        self.format
    }
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.inner.entries()
    }
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        let copied = self.inner.extract(entry, writer)?;
        if !entry.is_dir && copied != entry.size {
            return Err(GeeZipError::format(
                "truncated optical image entry",
                self.format,
            ));
        }
        Ok(copied)
    }
}

const CD_SYNC: [u8; 12] = [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];

fn sector_offset(header: &[u8; 24]) -> io::Result<u64> {
    if header[..12] != CD_SYNC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid CD sector sync",
        ));
    }
    match header[15] {
        1 => Ok(16),
        2 if header[16..20] == header[20..24] && header[18] & 0x20 == 0 => Ok(24),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported CD sector mode",
        )),
    }
}

struct Sectors<R> {
    inner: R,
    pos: u64,
    len: u64,
    stride: u64,
    offset: u64,
    physical_pos: u64,
    validated_sector: Option<u64>,
}
impl<R: Read + Seek> Sectors<R> {
    fn new(mut inner: R) -> io::Result<Self> {
        let physical_len = inner.seek(SeekFrom::End(0))?;
        inner.seek(SeekFrom::Start(0))?;
        let mut header = [0; 24];
        let read = physical_len.min(header.len() as u64) as usize;
        inner.read_exact(&mut header[..read])?;
        let raw = read == 24 && header[..12] == CD_SYNC;
        let (stride, offset) = if raw {
            let offset = sector_offset(&header)?;
            if !physical_len.is_multiple_of(2352) {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partial raw CD sector",
                ));
            }
            (2352, offset)
        } else {
            (2048, 0)
        };
        let len = if raw {
            physical_len / 2352 * 2048
        } else {
            physical_len
        };
        inner.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner,
            pos: 0,
            len,
            stride,
            offset,
            physical_pos: 0,
            validated_sector: None,
        })
    }
    fn seek_physical(&mut self, position: u64) -> io::Result<()> {
        let delta = i64::try_from(position as i128 - self.physical_pos as i128)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "image seek overflow"))?;
        if delta != 0 {
            self.inner.seek_relative(delta)?;
        }
        self.physical_pos = position;
        Ok(())
    }
}
impl<R: Read + Seek> Read for Sectors<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let sector = self.pos / 2048;
        if self.stride == 2352 && self.validated_sector != Some(sector) {
            self.seek_physical(sector * self.stride)?;
            let mut header = [0; 24];
            self.inner.read_exact(&mut header)?;
            self.physical_pos += 24;
            if sector_offset(&header)? != self.offset {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "mixed CD sector modes",
                ));
            }
            self.validated_sector = Some(sector);
        }
        let physical = sector * self.stride + self.offset + self.pos % 2048;
        self.seek_physical(physical)?;
        let remaining = if self.stride == 2048 {
            self.len - self.pos
        } else {
            (self.len - self.pos).min(2048 - self.pos % 2048)
        };
        let n = buf.len().min(remaining.min(usize::MAX as u64) as usize);
        let read = self.inner.read(&mut buf[..n])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated image",
            ));
        }
        self.pos += read as u64;
        self.physical_pos += read as u64;
        Ok(read)
    }
}
impl<R: Read + Seek> Seek for Sectors<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.pos as i128 + n as i128,
            SeekFrom::End(n) => self.len as i128 + n as i128,
        };
        self.pos = u64::try_from(next)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid image seek"))?;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{iso::IsoWriter, ArchiveWriter};
    use super::*;
    #[test]
    fn cooked_and_raw_optical_images_extract_actual_files() {
        let mut iso = IsoWriter::new(Vec::new());
        iso.add_entry_from_reader(
            Path::new("nested/payload.txt"),
            &mut b"image contents".as_slice(),
        )
        .unwrap();
        let (_, data) = iso.finalize().unwrap();
        for mode in [0, 1, 2] {
            let image = if mode == 0 {
                data.clone()
            } else {
                let mut raw = Vec::new();
                for sector in data.chunks(2048) {
                    let mut block = [0; 2352];
                    block[..12]
                        .copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
                    block[15] = mode;
                    let offset = if mode == 1 { 16 } else { 24 };
                    block[offset..offset + sector.len()].copy_from_slice(sector);
                    raw.extend_from_slice(&block);
                }
                raw
            };
            let file = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(file.path(), image).unwrap();
            let mut reader = DiskImageReader::open(file.path(), ArchiveFormat::Bin).unwrap();
            let entry = reader
                .entries()
                .unwrap()
                .into_iter()
                .find(|e| e.path.ends_with("payload.txt"))
                .unwrap();
            let mut out = Vec::new();
            reader.extract(&entry, &mut out).unwrap();
            assert_eq!(out, b"image contents");
        }
    }
    #[test]
    fn raw_sectors_handle_short_reads_and_validate_each_header() {
        struct ShortReads(std::io::Cursor<Vec<u8>>);
        impl Read for ShortReads {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = buf.len().min(1);
                self.0.read(&mut buf[..n])
            }
        }
        impl Seek for ShortReads {
            fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
                self.0.seek(pos)
            }
        }
        let mut block = [0; 2352];
        block[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
        block[15] = 1;
        block[16..2064].fill(0x5a);
        let mut data = block.to_vec();
        data.extend(block);
        let mut view = Sectors::new(ShortReads(std::io::Cursor::new(data.clone()))).unwrap();
        let mut output = Vec::new();
        view.read_to_end(&mut output).unwrap();
        assert_eq!(output, vec![0x5a; 4096]);

        for (position, value) in [(2352, 1), (2352 + 15, 2)] {
            let mut invalid = data.clone();
            invalid[position] = value;
            let mut view = Sectors::new(std::io::Cursor::new(invalid)).unwrap();
            assert!(view.read_to_end(&mut Vec::new()).is_err());
            // A direct seek must validate the target sector as well.
            view.seek(SeekFrom::Start(2049)).unwrap();
            assert!(view.read(&mut [0; 1]).is_err());
        }
    }

    #[test]
    fn random_bytes_are_not_reported_as_extractable_images() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), [0; 8192]).unwrap();
        assert!(DiskImageReader::open(file.path(), ArchiveFormat::Bin)
            .unwrap()
            .entries()
            .is_err());
    }
}
