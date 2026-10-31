//! Single-volume, unencrypted PEA 1.x reader (Store and PCOMPRESS1–3).
//!
//! PEA has no central index: opening scans and checks each object and the stream
//! and volume tags. Later extraction seeks directly to the selected object and
//! rechecks it. Payloads use fixed buffers; source contents must remain stable.
mod checksum;
mod input;
mod paths;
mod record;

use super::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult as Result};
use checksum::bad;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

const METADATA_BUDGET: usize = 64 * 1024 * 1024;
trait Source: Read + Seek + Send {}
impl<T: Read + Seek + Send> Source for T {}
#[derive(Clone, Copy)]
struct Settings {
    compression: u8,
    block_size: u32,
    object_hash: u8,
}
struct Indexed {
    entry: Entry,
    start: u64,
    end: u64,
    fingerprint: [u8; 32],
}

/// Bounded-buffer PEA reader. Legacy ANSI names must be valid UTF-8/ASCII;
/// encrypted/HMAC streams, split volumes and unknown schemes are rejected.
pub struct PeaReader {
    source: BufReader<Box<dyn Source>>,
    settings: Settings,
    index: Vec<Indexed>,
    lookup: HashMap<String, usize>,
    verified_bytes: u64,
}
impl PeaReader {
    pub fn open(path: &Path) -> Result<Self> {
        Self::new(File::open(path)?)
    }

    /// The supplied source receives empty reads while inflating so wrappers can
    /// poll cancellation even when a compressed block produces many output chunks.
    pub fn new<R: Read + Seek + Send + 'static>(mut source: R) -> Result<Self> {
        let length = source.seek(SeekFrom::End(0))?;
        source.rewind()?;
        let mut source: BufReader<Box<dyn Source>> =
            BufReader::with_capacity(64 * 1024, Box::new(source));
        let mut header = [0; 20];
        source.read_exact(&mut header)?;
        if header[0] != 0xea
            || header[1] != 1
            || header[2] > 6
            || header[10..16] != [0, 0, b'P', b'O', b'D', 0]
        {
            return Err(bad("invalid or unsupported PEA header"));
        }
        if header[4] != 0 || header[17] != 0 || header[7] != 1 || header[8] & 0x80 != 0 {
            return Err(bad("unsupported ECC, filename encoding or byte order"));
        }
        if header[18] >= 0x30 {
            return Err(GeeZipError::Crypto {
                message: "password-based PEA verification/encryption is not supported".into(),
            });
        }
        checksum::Checksum::new(header[19])?;
        let mut settings = Settings {
            compression: header[16],
            block_size: 0,
            object_hash: header[19],
        };
        if settings.compression > 3 {
            return Err(bad("unsupported compression scheme"));
        }
        let mut input = input::Input::new(&mut source, length, &header)?;
        if settings.compression != 0 {
            settings.block_size = u32::from_le_bytes(input.array::<4>()?);
            if settings.block_size == 0 {
                return Err(bad("zero compression buffer size"));
            }
        }
        let mut index = Vec::new();
        let mut metadata = 0;
        let mut verified_bytes = 0u64;
        loop {
            let start = input.position;
            let Some(record) = record::read(
                &mut input,
                &settings,
                &mut std::io::sink(),
                None,
                METADATA_BUDGET - metadata,
            )?
            else {
                break;
            };
            metadata += record.name.len() * 3 + 256;
            verified_bytes = verified_bytes
                .checked_add(record.size)
                .ok_or_else(|| bad("uncompressed byte count overflow"))?;
            index
                .try_reserve(1)
                .map_err(|_| bad("cannot allocate entry index"))?;
            index.push(Indexed {
                entry: Entry {
                    path: record.name,
                    size: record.size,
                    compressed_size: record.encoded_size,
                    crc32: None,
                    modified: None,
                    is_dir: record.is_dir,
                },
                start,
                end: input.position,
                fingerprint: record.fingerprint,
            });
        }
        input.finish_stream()?;
        paths::normalize(&mut index)?;
        let mut lookup = HashMap::new();
        lookup
            .try_reserve(index.len())
            .map_err(|_| bad("cannot allocate filename index"))?;
        for (position, item) in index.iter().enumerate() {
            if lookup.insert(item.entry.path.clone(), position).is_some() {
                return Err(bad("duplicate normalized filename"));
            }
        }
        for item in &index {
            let mut name = item.entry.path.as_str();
            while let Some((parent, _)) = name.rsplit_once('/') {
                if lookup
                    .get(parent)
                    .is_some_and(|&position| !index[position].entry.is_dir)
                {
                    return Err(bad("file is used as a directory ancestor"));
                }
                name = parent;
            }
        }
        Ok(Self {
            source,
            settings,
            index,
            lookup,
            verified_bytes,
        })
    }

    /// Opening verifies all payloads and available checksum layers; test callers
    /// can report this without decoding every entry a second time.
    pub fn verified_stats(&self) -> (u64, u64) {
        (self.index.len() as u64, self.verified_bytes)
    }
}
impl ArchiveReader for PeaReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Pea
    }
    fn entries(&mut self) -> Result<Vec<Entry>> {
        Ok(self.index.iter().map(|item| item.entry.clone()).collect())
    }
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> Result<u64> {
        let &position = self
            .lookup
            .get(&entry.path)
            .ok_or_else(|| GeeZipError::EntryNotFound {
                name: entry.path.clone(),
            })?;
        let item = &self.index[position];
        self.source.seek(SeekFrom::Start(item.start))?;
        let mut input = input::Input::record(&mut self.source, item.start, item.end)?;
        let record = record::read(
            &mut input,
            &self.settings,
            writer,
            Some(&item.fingerprint),
            METADATA_BUDGET,
        )?
        .ok_or_else(|| bad("entry replaced by archive terminator"))?;
        input.finish_record()?;
        Ok(record.size)
    }
}

#[cfg(test)]
mod tests;
