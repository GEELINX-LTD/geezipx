//! Bounded payload staging for one solid ZPAQ archive.
use crate::archive::{is_entry_path_dangerous, ArchiveWriter};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};

const BUFFER: usize = 64 * 1024;
const METADATA_BUDGET: usize = 64 * 1024 * 1024;
struct StagedEntry {
    path: String,
    offset: u64,
    size: u64,
}

/// ZPAQ writer preserving a shared compression model across named segments.
/// Payloads use one anonymous temporary file; only bounded metadata is resident.
/// libzpaq's model memory depends on the selected built-in level, not input size.
pub struct ZpaqWriter<W: Write + Send> {
    inner: Option<W>,
    staging: Option<File>,
    entries: Vec<StagedEntry>,
    paths: BTreeMap<String, bool>,
    staged_bytes: u64,
    metadata_bytes: usize,
    poisoned: bool,
    pub(super) method: String,
}
impl<W: Write + Send> fmt::Debug for ZpaqWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZpaqWriter")
            .field("method", &self.method)
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}
fn invalid(message: impl Into<String>) -> GeeZipError {
    GeeZipError::format(message, ArchiveFormat::Zpaq)
}

impl<W: Write + Send> ZpaqWriter<W> {
    pub fn new(writer: W, level: Option<u32>) -> Self {
        // libzpaq exposes three built-in solid streaming models. Compatibility
        // levels 4+ select model 3; upstream's numeric 4/5 would fail at runtime.
        let method = match level {
            None | Some(0..=1) => "1",
            Some(2) => "2",
            Some(_) => "3",
        };
        Self {
            inner: Some(writer),
            staging: None,
            entries: Vec::new(),
            paths: BTreeMap::new(),
            staged_bytes: 0,
            metadata_bytes: 0,
            poisoned: false,
            method: method.into(),
        }
    }
    fn prepare_path(&self, path: &Path, directory: bool) -> GeeZipResult<Option<(String, usize)>> {
        if self.poisoned {
            return Err(invalid("ZPAQ staging is unusable after an I/O failure"));
        }
        let raw = path
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 ZPAQ path"))?;
        if raw.len() > 64 * 1024 {
            return Err(invalid("ZPAQ path exceeds 64 KiB limit"));
        }
        let normalized = raw.replace('\\', "/");
        if normalized.contains('\0') || is_entry_path_dangerous(Path::new(&normalized)) {
            return Err(invalid(format!("invalid ZPAQ entry path: {raw}")));
        }
        if !directory && normalized.ends_with('/') {
            return Err(invalid("ZPAQ file path ends with slash"));
        }
        let mut name = String::with_capacity(normalized.len());
        let mut depth = 0usize;
        for component in Path::new(&normalized).components() {
            if let Component::Normal(part) = component {
                depth += 1;
                if depth > 256 {
                    return Err(invalid("ZPAQ path exceeds 256 components"));
                }
                if !name.is_empty() {
                    name.push('/');
                }
                name.push_str(part.to_str().expect("input path is UTF-8"));
            }
        }
        name.shrink_to_fit();
        if name.is_empty() {
            return Err(invalid("empty ZPAQ entry path"));
        }
        if let Some(existing) = self.paths.get(&name) {
            if directory && *existing {
                return Ok(None);
            }
            return Err(invalid(format!(
                "duplicate or conflicting ZPAQ path: {name}"
            )));
        }
        for (pos, _) in name.match_indices('/') {
            if self.paths.get(&name[..pos]) == Some(&false) {
                return Err(invalid("ZPAQ parent is a file"));
            }
        }
        if !directory {
            let prefix = format!("{name}/");
            if self
                .paths
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(p, _)| p.starts_with(&prefix))
            {
                return Err(invalid("ZPAQ file conflicts with existing children"));
            }
        }
        // Include both copies of the name, entry metadata and tree overhead.
        let cost = name
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| invalid("ZPAQ metadata size overflow"))?;
        if self
            .metadata_bytes
            .checked_add(cost)
            .is_none_or(|n| n > METADATA_BUDGET)
        {
            return Err(invalid("ZPAQ metadata exceeds 64 MiB budget"));
        }
        Ok(Some((name, cost)))
    }
    fn register(&mut self, name: String, directory: bool, offset: u64, size: u64, cost: usize) {
        self.paths.insert(name.clone(), directory);
        self.entries.push(StagedEntry {
            path: if directory { format!("{name}/") } else { name },
            offset,
            size,
        });
        self.metadata_bytes += cost;
    }
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        if self.poisoned {
            return Err(invalid("ZPAQ staging is unusable after an I/O failure"));
        }
        let mut writer = self
            .inner
            .take()
            .ok_or_else(|| invalid("ZPAQ writer already finalised"))?;
        let mut output = CountedOutput {
            inner: &mut writer,
            count: 0,
        };
        if !self.entries.is_empty() {
            let source = Arc::new(Mutex::new(match self.staging.take() {
                Some(file) => file,
                None => tempfile::tempfile()
                    .map_err(|e| GeeZipError::io(e, "creating ZPAQ staging file"))?,
            }));
            let entries = self
                .entries
                .into_iter()
                .map(|e| zpaq_rs::ReaderArchiveEntry {
                    path: e.path,
                    reader: BufReader::with_capacity(
                        BUFFER,
                        StagedRange {
                            file: Arc::clone(&source),
                            position: e.offset,
                            remaining: e.size,
                        },
                    ),
                    size: e.size,
                    comment: None,
                });
            drop(self.paths);
            zpaq_rs::archive_from_reader_entries(entries, &mut output, &self.method)
                .map_err(|e| invalid(format!("ZPAQ compression failed: {e}")))?;
        }
        output
            .flush()
            .map_err(|e| GeeZipError::io(e, "flushing ZPAQ archive"))?;
        let count = output.count;
        Ok((count, writer))
    }
}
impl<W: Write + Send> ArchiveWriter for ZpaqWriter<W> {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Zpaq
    }
    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let (name, cost) = self
            .prepare_path(path, false)?
            .expect("file cannot be an existing directory");
        if self.staging.is_none() {
            self.staging = Some(
                tempfile::tempfile()
                    .map_err(|e| GeeZipError::io(e, "creating ZPAQ staging file"))?,
            );
        }
        let start = self.staged_bytes;
        let source = self.staging.as_mut().expect("staging created");
        let result = (|| -> io::Result<u64> {
            source.seek(SeekFrom::Start(start))?;
            let mut size = 0u64;
            let mut buf = [0u8; BUFFER];
            loop {
                let n = match reader.read(&mut buf) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                if n == 0 {
                    break;
                }
                size = size
                    .checked_add(n as u64)
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or_else(|| io::Error::other("ZPAQ member size overflow"))?;
                start
                    .checked_add(size)
                    .ok_or_else(|| io::Error::other("ZPAQ staging size overflow"))?;
                source.write_all(&buf[..n])?;
            }
            Ok(size)
        })();
        let size = match result {
            Ok(size) => size,
            Err(error) => {
                if source
                    .set_len(start)
                    .and_then(|_| source.seek(SeekFrom::Start(start)))
                    .is_err()
                {
                    self.poisoned = true;
                }
                return Err(GeeZipError::io(
                    error,
                    format!("staging ZPAQ entry '{name}'"),
                ));
            }
        };
        self.staged_bytes += size;
        self.register(name, false, start, size, cost);
        Ok(())
    }
    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        if let Some((name, cost)) = self.prepare_path(path, true)? {
            self.register(name, true, self.staged_bytes, 0, cost);
        }
        Ok(())
    }
    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        (*self).finalize().map(|(size, _)| size)
    }
}

struct StagedRange {
    file: Arc<Mutex<File>>,
    position: u64,
    remaining: u64,
}
impl Read for StagedRange {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.remaining.min(buf.len() as u64).min(BUFFER as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("ZPAQ staging lock poisoned"))?;
        file.seek(SeekFrom::Start(self.position))?;
        let n = file.read(&mut buf[..count])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated ZPAQ staging file",
            ));
        }
        self.position += n as u64;
        self.remaining -= n as u64;
        Ok(n)
    }
}
struct CountedOutput<W> {
    inner: W,
    count: u64,
}
impl<W: Write> Write for CountedOutput<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count = self
            .count
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("ZPAQ output size overflow"))?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests;
