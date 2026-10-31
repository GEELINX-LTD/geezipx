//! ASAR creation with a disk-backed payload and a bounded JSON index.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::archive::{is_entry_path_dangerous, ArchiveWriter, CountWriter};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

const BUFFER: usize = 64 * 1024;
const HASH_BLOCK: usize = 4 * 1024 * 1024;
const INDEX_BUDGET: usize = 64 * 1024 * 1024;

/// Snapshot arbitrary inputs to one temporary file. Only names and integrity
/// metadata remain in RAM; output needs neither seeking nor payload buffering.
pub struct AsarWriter<W: Write + Send> {
    writer: W,
    payload: Option<File>,
    entries: BTreeMap<String, Option<FileInfo>>,
    index_cost: usize,
    failed: bool,
}

#[derive(Serialize)]
struct FileInfo {
    size: u64,
    offset: String,
    integrity: Integrity,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Integrity {
    algorithm: &'static str,
    hash: String,
    block_size: usize,
    blocks: Vec<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Node {
    Directory { files: BTreeMap<String, Node> },
    File(FileInfo),
}

impl<W: Write + Send> std::fmt::Debug for AsarWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsarWriter")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> AsarWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            payload: None,
            entries: BTreeMap::new(),
            index_cost: 0,
            failed: false,
        }
    }

    fn check_path(&self, path: &Path, directory: bool) -> GeeZipResult<(String, usize)> {
        if self.failed {
            return Err(error("ASAR staging file unusable after I/O failure"));
        }
        let raw = path.to_str().ok_or_else(|| error("non-UTF-8 ASAR path"))?;
        let path = raw.replace('\\', "/");
        let path = if directory {
            path.trim_end_matches('/')
        } else {
            &path
        };
        if path.is_empty()
            || path.contains('\0')
            || is_entry_path_dangerous(Path::new(path))
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
            || path.split('/').count() > 48
        {
            return Err(error("invalid ASAR path or directory depth exceeds 48"));
        }
        if let Some(existing) = self.entries.get(path) {
            if !directory || existing.is_some() {
                return Err(error("duplicate or conflicting ASAR entry"));
            }
        }
        let mut cost = self.index_cost;
        let mut current = path;
        loop {
            if let Some(existing) = self.entries.get(current) {
                if current != path && existing.is_some() {
                    return Err(error("ASAR parent is a file"));
                }
            } else {
                // Covers escaped names, tree bookkeeping and fixed JSON fields.
                cost = cost
                    .checked_add(current.len().saturating_mul(8).saturating_add(256))
                    .filter(|n| *n <= INDEX_BUDGET)
                    .ok_or_else(|| error("ASAR index exceeds 64 MiB budget"))?;
            }
            if let Some((parent, _)) = current.rsplit_once('/') {
                current = parent;
            } else {
                break;
            }
        }
        Ok((path.to_owned(), cost))
    }

    fn register(&mut self, path: String, info: Option<FileInfo>, cost: usize) {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            self.entries.entry(prefix.to_owned()).or_insert(None);
            parent = prefix;
        }
        self.entries.insert(path, info);
        self.index_cost = cost;
    }

    /// Finish the pickle header and stream the staged payload, returning output.
    pub fn finalize(self) -> GeeZipResult<(u64, W)> {
        if self.failed {
            return Err(error("cannot finalize a failed ASAR writer"));
        }
        let mut children: BTreeMap<String, BTreeMap<String, Node>> = BTreeMap::new();
        for (path, info) in self.entries.into_iter().rev() {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", &path));
            let node = match info {
                Some(info) => Node::File(info),
                None => Node::Directory {
                    files: children.remove(&path).unwrap_or_default(),
                },
            };
            children
                .entry(parent.to_owned())
                .or_default()
                .insert(name.to_owned(), node);
        }
        let header = Node::Directory {
            files: children.remove("").unwrap_or_default(),
        };
        let json = serde_json::to_vec(&header)
            .map_err(|err| error(format!("serializing ASAR index: {err}")))?;
        if json.len() > INDEX_BUDGET {
            return Err(error("ASAR JSON index exceeds 64 MiB"));
        }
        let length = u32::try_from(json.len()).map_err(|_| error("ASAR JSON size overflow"))?;
        let aligned = (length + 3) & !3;
        let mut output = CountWriter {
            inner: self.writer,
            count: 0,
        };
        for number in [4u32, aligned + 8, aligned + 4, length] {
            output.write_all(&number.to_le_bytes())?;
        }
        output.write_all(&json)?;
        output.write_all(&[0u8; 3][..(aligned - length) as usize])?;
        if let Some(mut payload) = self.payload {
            let expected = payload.metadata()?.len();
            payload.rewind()?;
            let mut buffer = [0u8; BUFFER];
            let mut copied = 0u64;
            loop {
                let n = match payload.read(&mut buffer) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                if n == 0 {
                    break;
                }
                output.write_all(&buffer[..n])?;
                copied += n as u64;
            }
            if copied != expected {
                return Err(error("truncated ASAR staging file"));
            }
        }
        output.flush()?;
        Ok((output.count, output.inner))
    }
}

impl<W: Write + Send> ArchiveWriter for AsarWriter<W> {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Asar
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let (path, mut cost) = self.check_path(path, false)?;
        if self.payload.is_none() {
            self.payload = Some(tempfile::tempfile()?);
        }
        let payload = self.payload.as_mut().expect("staging file initialized");
        let offset = payload.seek(SeekFrom::End(0))?;
        let result = (|| -> GeeZipResult<FileInfo> {
            let mut buffer = [0u8; BUFFER];
            let mut size = 0u64;
            let mut hash = Sha256::new();
            let mut block_hash = Sha256::new();
            let mut in_block = 0;
            let mut blocks = Vec::new();
            loop {
                let n = match reader.read(&mut buffer) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                if n == 0 {
                    break;
                }
                size = size
                    .checked_add(n as u64)
                    .filter(|n| offset.checked_add(*n).is_some_and(|end| end < (1u64 << 53)))
                    .ok_or_else(|| error("ASAR file size exceeds exact JSON integer range"))?;
                hash.update(&buffer[..n]);
                let mut remaining = &buffer[..n];
                while !remaining.is_empty() {
                    let take = remaining.len().min(HASH_BLOCK - in_block);
                    block_hash.update(&remaining[..take]);
                    remaining = &remaining[take..];
                    in_block += take;
                    if in_block == HASH_BLOCK {
                        cost = add_hash_cost(cost)?;
                        blocks.push(super::bytes_to_hex(&block_hash.finalize_reset()));
                        in_block = 0;
                    }
                }
                payload.write_all(&buffer[..n])?;
            }
            // Electron records a final block, including the empty block at EOF.
            cost = add_hash_cost(cost)?;
            blocks.push(super::bytes_to_hex(&block_hash.finalize()));
            Ok(FileInfo {
                size,
                offset: offset.to_string(),
                integrity: Integrity {
                    algorithm: "SHA256",
                    hash: super::bytes_to_hex(&hash.finalize()),
                    block_size: HASH_BLOCK,
                    blocks,
                },
            })
        })();
        match result {
            Ok(info) => {
                self.register(path, Some(info), cost);
                Ok(())
            }
            Err(err) => {
                if payload
                    .set_len(offset)
                    .and_then(|_| payload.seek(SeekFrom::Start(offset)))
                    .is_err()
                {
                    self.failed = true;
                }
                Err(err)
            }
        }
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let (path, cost) = self.check_path(path, true)?;
        self.register(path, None, cost);
        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        self.finalize().map(|(size, _)| size)
    }
}

fn add_hash_cost(cost: usize) -> GeeZipResult<usize> {
    cost.checked_add(96)
        .filter(|n| *n <= INDEX_BUDGET)
        .ok_or_else(|| error("ASAR integrity index exceeds 64 MiB budget"))
}
fn error(message: impl Into<String>) -> GeeZipError {
    GeeZipError::format(message, ArchiveFormat::Asar)
}

#[cfg(test)]
mod tests;
