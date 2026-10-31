//! Bounded EGG index parsing. Payloads are skipped, never allocated here.
use super::error;
use crate::error::GeeZipResult;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use unegg::archive::{CompressionMethod, EggBlock, EggFileEntry};

const END: u32 = 0x08e28222;
const FILE: u32 = 0x0a8590e3;
const BLOCK: u32 = 0x02b50c13;
const NAME: u32 = 0x0a8591ac;
const COMMENT: u32 = 0x04c63672;
const DUMMY: u32 = 0x07463307;
const MAX_METADATA: usize = 64 * 1024 * 1024;

struct Index<'a, R> {
    reader: &'a mut R,
    end: u64,
    budget: usize,
}
impl<R: Read + Seek> Index<'_, R> {
    fn charge(&mut self, size: usize) -> GeeZipResult<()> {
        self.budget = self
            .budget
            .checked_sub(size)
            .ok_or_else(|| error("index exceeds 64 MiB metadata budget"))?;
        Ok(())
    }
    fn bytes<const N: usize>(&mut self) -> GeeZipResult<[u8; N]> {
        self.charge(N)?;
        let mut bytes = [0; N];
        self.reader.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    fn u32(&mut self) -> GeeZipResult<u32> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn extra(&mut self) -> GeeZipResult<(u8, u64)> {
        let flags = self.bytes::<1>()?[0];
        let size = if flags & 1 != 0 {
            u64::from(self.u32()?)
        } else {
            u64::from(u16::from_le_bytes(self.bytes()?))
        };
        self.range(size)?;
        Ok((flags, size))
    }
    fn range(&mut self, size: u64) -> GeeZipResult<u64> {
        self.reader
            .stream_position()?
            .checked_add(size)
            .filter(|&end| end <= self.end)
            .ok_or_else(|| error("header or payload extends beyond archive"))
    }
    fn skip(&mut self, size: u64) -> GeeZipResult<()> {
        let end = self.range(size)?;
        self.reader.seek(SeekFrom::Start(end))?;
        Ok(())
    }
    fn entry(&mut self) -> GeeZipResult<EggFileEntry> {
        // Include container/index allocations as well as bytes read in the budget.
        self.charge(std::mem::size_of::<EggFileEntry>() + 64)?;
        let mut entry = EggFileEntry {
            file_id: self.u32()?,
            uncompressed_size: u64::from_le_bytes(self.bytes()?),
            file_name: String::new(),
            blocks: Vec::new(),
            encrypt_info: None,
            file_time: None,
            file_attr: 0,
            parent_id: None,
        };
        let mut has_name = false;
        loop {
            let signature = self.u32()?;
            if signature == END {
                break;
            }
            let (flags, size) = self.extra()?;
            match signature {
                NAME => {
                    if has_name || flags & 4 != 0 {
                        return Err(error("duplicate or encrypted filename"));
                    }
                    let mut remaining = size;
                    let locale = if flags & 8 != 0 {
                        remaining = remaining
                            .checked_sub(2)
                            .ok_or_else(|| error("short locale"))?;
                        Some(u16::from_le_bytes(self.bytes()?))
                    } else {
                        None
                    };
                    if flags & 0x10 != 0 {
                        remaining = remaining
                            .checked_sub(4)
                            .ok_or_else(|| error("short parent ID"))?;
                        entry.parent_id = Some(self.u32()?);
                    }
                    let size = usize::try_from(remaining).map_err(error)?;
                    // Legacy name decoding may expand one byte to three UTF-8 bytes.
                    self.charge(
                        size.checked_mul(4)
                            .ok_or_else(|| error("name size overflow"))?,
                    )?;
                    let mut name = vec![0; size];
                    self.reader.read_exact(&mut name)?;
                    // Keep absolute/traversal paths intact for shared path validation.
                    entry.file_name =
                        unegg::encoding::decode_filename(flags & 8 != 0, locale, &name)
                            .replace('\\', "/");
                    if entry.file_name.is_empty() || entry.file_name.chars().any(char::is_control) {
                        return Err(error("empty filename or control character"));
                    }
                    has_name = true;
                }
                0x2c86950b => {
                    if size < 9 {
                        return Err(error("short Windows file info"));
                    }
                    entry.file_time = Some(u64::from_le_bytes(self.bytes()?));
                    entry.file_attr |= self.bytes::<1>()?[0];
                    self.skip(size - 9)?;
                }
                0x1ee922e5 => {
                    if size < 20 {
                        return Err(error("short POSIX file info"));
                    }
                    let mode = self.u32()?;
                    self.skip(8)?; // uid, gid
                    let mtime = u64::from_le_bytes(self.bytes()?);
                    if mode & 0xf000 == 0x4000 {
                        entry.file_attr |= 0x80;
                    } else if mode & 0xf000 != 0x8000 {
                        return Err(error("unsupported POSIX special file"));
                    }
                    entry.file_time.get_or_insert(
                        mtime
                            .saturating_add(11_644_473_600)
                            .saturating_mul(10_000_000),
                    );
                    self.skip(size - 20)?;
                }
                COMMENT | DUMMY => self.skip(size)?,
                0x08d1470f => return Err(error("encrypted entries are not supported")),
                _ => return Err(error("unsupported file header")),
            }
        }
        if !has_name {
            return Err(error("missing filename"));
        }
        let mut total_size = 0u64;
        loop {
            let signature = self.u32()?;
            if signature != BLOCK {
                self.reader.seek(SeekFrom::Current(-4))?;
                break;
            }
            self.charge(std::mem::size_of::<EggBlock>() * 2)?;
            let [method, _hint] = self.bytes()?;
            let compression_method = match method {
                0 => CompressionMethod::Store,
                1 => CompressionMethod::Deflate,
                2 => CompressionMethod::Bzip2,
                4 => CompressionMethod::Lzma,
                _ => return Err(error(format!("unsupported compression method {method}"))),
            };
            let uncompressed_size = self.u32()?;
            let compressed_size = self.u32()?;
            let crc32 = self.u32()?;
            if self.u32()? != END {
                return Err(error("missing block header terminator"));
            }
            let data_pos = self.reader.stream_position()?;
            self.skip(u64::from(compressed_size))?;
            total_size = total_size
                .checked_add(u64::from(uncompressed_size))
                .ok_or_else(|| error("file size overflow"))?;
            entry.blocks.push(EggBlock {
                compression_method,
                uncompressed_size,
                compressed_size,
                crc32,
                data_pos,
            });
        }
        if total_size != entry.uncompressed_size || (entry.is_directory() && total_size != 0) {
            return Err(error("file/block size mismatch"));
        }
        Ok(entry)
    }
}

pub(super) fn parse<R: Read + Seek>(reader: &mut R) -> GeeZipResult<Vec<EggFileEntry>> {
    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut index = Index {
        reader,
        end,
        budget: MAX_METADATA,
    };
    if index.u32()? != 0x41474745 || u16::from_le_bytes(index.bytes()?) != 256 {
        return Err(error("unsupported EGG signature or version"));
    }
    index.skip(8)?; // archive ID, reserved
    loop {
        match index.u32()? {
            END => break,
            0xffff0000 => {
                let (_, size) = index.extra()?;
                index.skip(size)?;
            }
            _ => return Err(error("solid, split, encrypted or unknown archive header")),
        }
    }
    let mut entries = Vec::new();
    loop {
        match index.u32()? {
            FILE => entries.push(index.entry()?),
            COMMENT | DUMMY => {
                let (_, size) = index.extra()?;
                index.skip(size)?;
            }
            END if index.reader.stream_position()? == end => break,
            END => {} // Some producers put an extra marker between files.
            _ => return Err(error("unexpected archive record")),
        }
    }
    let by_id: HashMap<_, _> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.file_id, i))
        .collect();
    if by_id.len() != entries.len() {
        return Err(error("duplicate file ID"));
    }
    // Resolve arbitrarily deep parents without recursion; charge every expanded name.
    let mut state = vec![0u8; entries.len()];
    let mut pending = Vec::new();
    for start in 0..entries.len() {
        let mut current = start;
        while state[current] != 2 {
            if state[current] == 1 {
                return Err(error("cyclic relative path"));
            }
            state[current] = 1;
            pending.push(current);
            match entries[current].parent_id {
                Some(id) => current = *by_id.get(&id).ok_or_else(|| error("missing parent ID"))?,
                None => break,
            }
        }
        while let Some(i) = pending.pop() {
            if let Some(parent) = entries[i].parent_id {
                let prefix = entries[by_id[&parent]].file_name.trim_end_matches('/');
                let size = prefix
                    .len()
                    .checked_add(1)
                    .and_then(|n| n.checked_add(entries[i].file_name.len()))
                    .ok_or_else(|| error("path length overflow"))?;
                index.charge(size)?;
                entries[i].file_name = format!("{prefix}/{}", entries[i].file_name);
            }
            state[i] = 2;
        }
    }
    Ok(entries)
}
