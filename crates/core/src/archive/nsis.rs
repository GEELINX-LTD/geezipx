//! Streaming modern NSIS installer reading; installer code is never executed.
mod decode;
mod index;
mod input;
use super::{ArchiveReader, Entry};
use crate::{
    detect::ArchiveFormat,
    error::{GeeZipError, GeeZipResult as Result},
};
pub(crate) use input::probe;
use nsis::decompress::{CompressionMethod, CompressionMode};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};

const METADATA_BUDGET: usize = 64 * 1024 * 1024;
trait Source: Read + Seek + Send {}
impl<T: Read + Seek + Send> Source for T {}
type Input<'a> = std::io::Take<&'a mut BufReader<Box<dyn Source>>>;
fn bad(message: impl Into<String>) -> GeeZipError {
    GeeZipError::format(message, ArchiveFormat::Nsis)
}
struct Indexed {
    entry: Entry,
    start: u64,
    encoded_size: u64,
    method: CompressionMethod,
    fingerprint: [u8; 32],
}

/// Read-only NSIS 3.x reader with bounded codec buffers and a disk-backed
/// solid stream. Lists embedded files and static directories; it does not
/// emulate script branches, run plugins, or reconstruct generated uninstallers.
pub struct NsisReader {
    source: BufReader<Box<dyn Source>>,
    index: Vec<Indexed>,
    lookup: HashMap<String, usize>,
    verified_bytes: u64,
    crc_verified: bool,
}
impl NsisReader {
    /// Opens and verifies a modern NSIS installer.
    pub fn open(path: &Path) -> Result<Self> {
        Self::new(File::open(path)?)
    }
    /// Opens a seekable source. Metadata uses a 64 MiB budget, LZMA dictionaries
    /// are capped at 64 MiB, and solid streams are decoded to an anonymous file.
    pub fn new<R: Read + Seek + Send + 'static>(source: R) -> Result<Self> {
        let mut source: BufReader<Box<dyn Source>> =
            BufReader::with_capacity(64 * 1024, Box::new(source));
        let layout = input::layout(&mut source)?;
        input::verify_crc(&mut source, &layout)?;
        let (header, method, mode, base, end) = input::read_header(&mut source, &layout)?;
        let metadata_size = header.len();
        let metadata = nsis::NsisInstaller::from_decoded_header(
            header,
            layout.first,
            method,
            mode,
            &layout.stub_hints,
        )
        .map_err(|error| bad(error.to_string()))?;
        if metadata.version() != nsis::opcode::NsisVersion::V3
            || metadata.string_encoding() != nsis::strings::StringEncoding::Unicode
        {
            return Err(bad(
                "only modern NSIS 3.x Unicode is supported; ANSI, legacy and Park installers are not supported",
            ));
        }
        let mut index = index::build(&metadata, metadata_size, base, end)?;
        let mut verified_bytes = 0u64;
        for item in &mut index {
            if item.entry.is_dir {
                continue;
            }
            source.seek(SeekFrom::Start(item.start))?;
            let mut prefix = [0; 4];
            source.read_exact(&mut prefix)?;
            let raw = u32::from_le_bytes(prefix);
            item.encoded_size = u64::from(raw & 0x7fff_ffff);
            item.start = item
                .start
                .checked_add(4)
                .ok_or_else(|| bad("data offset overflow"))?;
            if item
                .start
                .checked_add(item.encoded_size)
                .is_none_or(|value| value > end)
            {
                return Err(bad("file data exceeds installer bounds"));
            }
            if mode == CompressionMode::Solid && raw & 0x8000_0000 != 0 {
                return Err(bad("compressed sub-block inside decoded solid stream"));
            }
            item.method = if raw & 0x8000_0000 == 0 {
                CompressionMethod::None
            } else if method == CompressionMethod::None {
                input::guess(&mut source)?
            } else {
                method
            };
            source.seek(SeekFrom::Start(item.start))?;
            let mut hash = HashWriter {
                output: &mut std::io::sink(),
                hash: Sha256::new(),
            };
            item.entry.size = decode::run(
                item.method,
                &mut source.by_ref().take(item.encoded_size),
                &mut hash,
                u64::from(u32::MAX),
            )?;
            item.entry.compressed_size = if mode == CompressionMode::Solid {
                0
            } else {
                item.encoded_size
            };
            item.fingerprint = hash.hash.finalize().into();
            verified_bytes = verified_bytes
                .checked_add(item.entry.size)
                .ok_or_else(|| bad("byte count overflow"))?;
        }
        let mut lookup = HashMap::new();
        lookup
            .try_reserve(index.len())
            .map_err(|_| bad("cannot allocate filename index"))?;
        for (position, item) in index.iter().enumerate() {
            lookup.insert(item.entry.path.clone(), position);
        }
        Ok(Self {
            source,
            index,
            lookup,
            verified_bytes,
            crc_verified: layout.crc,
        })
    }
    /// Opening checks every embedded payload and the optional installer CRC.
    /// The last field reports a container CRC, not a per-file checksum.
    pub fn verified_stats(&self) -> (u64, u64, bool) {
        (
            self.index.len() as u64,
            self.verified_bytes,
            self.crc_verified,
        )
    }
}
impl ArchiveReader for NsisReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Nsis
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
        if item.entry.is_dir {
            return Ok(0);
        }
        self.source.seek(SeekFrom::Start(item.start))?;
        let mut output = HashWriter {
            output: writer,
            hash: Sha256::new(),
        };
        let count = decode::run(
            item.method,
            &mut self.source.by_ref().take(item.encoded_size),
            &mut output,
            item.entry.size,
        )?;
        if count != item.entry.size || <[u8; 32]>::from(output.hash.finalize()) != item.fingerprint
        {
            return Err(bad("payload changed after the installer was opened"));
        }
        Ok(count)
    }
}
struct HashWriter<'a> {
    output: &'a mut dyn Write,
    hash: Sha256,
}
impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.output.write(bytes)?;
        self.hash.update(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

#[cfg(test)]
mod tests;
