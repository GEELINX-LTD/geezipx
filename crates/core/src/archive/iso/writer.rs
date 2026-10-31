//! Disk-backed ISO creation for arbitrary (including non-seekable) outputs.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use hadris_iso::joliet::JolietLevel;
use hadris_iso::read::PathSeparator;
use hadris_iso::rrip::RripOptions;
use hadris_iso::write::options::{BaseIsoLevel, CreationFeatures, FormatOptions};
use hadris_iso::write::{File as HadrisFile, FileSource, InputFiles, IsoImageWriter};

use crate::archive::ArchiveWriter;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

const BUFFER_SIZE: usize = 64 * 1024;

#[cfg(test)]
mod tests;
const MAX_ENTRY_SIZE: u64 = u32::MAX as u64;

/// ISO 9660 writer with Joliet and Rock Ridge enabled.
///
/// Payloads are snapshotted into one anonymous temporary file, then formatted
/// into a second seekable temporary file. RAM scales with entry metadata, not
/// payload size. Temporary disk space is approximately input plus image size.
/// The final image is copied to the supplied `Write` without requiring `Seek`.
pub struct IsoWriter<W: Write + Send> {
    inner: W,
    input: Option<File>,
    entries: BTreeMap<String, StagedEntry>,
    failed: bool,
}

#[derive(Debug)]
enum StagedEntry {
    File { offset: u64, len: u64 },
    Directory,
}

impl<W: Write + Send> std::fmt::Debug for IsoWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsoWriter")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> IsoWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            inner: writer,
            input: None,
            entries: BTreeMap::new(),
            failed: false,
        }
    }

    fn check_path(&self, path: &Path, is_dir: bool) -> GeeZipResult<String> {
        if self.failed {
            return Err(GeeZipError::format(
                "ISO staging file is unusable after I/O failure",
                ArchiveFormat::Iso,
            ));
        }
        let path = super::validate_iso_entry_path(path)?;
        // Canonicalize harmless aliases before checking duplicates or parents.
        let path = Path::new(&path)
            .components()
            .filter_map(|part| match part {
                std::path::Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if path.is_empty() {
            return Err(GeeZipError::format(
                "invalid empty ISO entry path",
                ArchiveFormat::Iso,
            ));
        }
        if path.matches('/').count() + usize::from(is_dir) > 7 {
            return Err(GeeZipError::format(
                "directory depth exceeds ISO 9660 limit of 8",
                ArchiveFormat::Iso,
            ));
        }
        if let Some(existing) = self.entries.get(&path) {
            if !is_dir || !matches!(existing, StagedEntry::Directory) {
                return Err(GeeZipError::format(
                    format!("duplicate or conflicting ISO entry: {path}"),
                    ArchiveFormat::Iso,
                ));
            }
        }
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            if matches!(self.entries.get(prefix), Some(StagedEntry::File { .. })) {
                return Err(GeeZipError::format(
                    format!("ISO parent is a file: {prefix}"),
                    ArchiveFormat::Iso,
                ));
            }
            parent = prefix;
        }
        Ok(path)
    }

    fn register(&mut self, path: String, entry: StagedEntry) {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            self.entries
                .entry(prefix.to_owned())
                .or_insert(StagedEntry::Directory);
            parent = prefix;
        }
        self.entries.insert(path, entry);
    }

    /// Finalize and return the exact image size and original output writer.
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        if self.failed || self.entries.is_empty() {
            return Err(GeeZipError::format(
                "cannot finalize an empty or failed ISO writer",
                ArchiveFormat::Iso,
            ));
        }
        let source = self.input.take().map(|file| Arc::new(Mutex::new(file)));
        let mut children: BTreeMap<String, Vec<HadrisFile>> = BTreeMap::new();
        // Reverse lexical order visits each child before its parent, with no
        // recursive full-map scans or payload clones. Directory depth is bounded.
        for (path, entry) in self.entries.into_iter().rev() {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", &path));
            let name = Arc::new(name.to_owned());
            let file = match entry {
                StagedEntry::Directory => {
                    let mut files = children.remove(&path).unwrap_or_default();
                    files.reverse();
                    HadrisFile::Directory {
                        name,
                        children: files,
                    }
                }
                StagedEntry::File { offset, len } => HadrisFile::Source {
                    name,
                    contents: FileSource::new(
                        source.as_ref().expect("staged input").clone(),
                        offset,
                        len,
                    )?,
                },
            };
            children.entry(parent.to_owned()).or_default().push(file);
        }
        let mut files = children.remove("").unwrap_or_default();
        files.reverse();
        let input = InputFiles {
            path_separator: PathSeparator::ForwardSlash,
            files,
        };
        let options = FormatOptions {
            volume_name: "GEEZIPX".into(),
            sector_size: 2048,
            system_id: None,
            volume_set_id: None,
            publisher_id: None,
            preparer_id: None,
            application_id: None,
            features: CreationFeatures {
                filenames: BaseIsoLevel::Level1 {
                    supports_lowercase: true,
                    supports_rrip: true,
                },
                long_filenames: false,
                joliet: Some(JolietLevel::Level3),
                rock_ridge: Some(RripOptions::default()),
                el_torito: None,
                hybrid_boot: None,
            },
            path_separator: PathSeparator::ForwardSlash,
            strict_charset: false,
        };
        let file = tempfile::tempfile()
            .map_err(|e| GeeZipError::io(e, "creating ISO image temporary file"))?;
        let mut image = ImageSpool {
            inner: BufWriter::with_capacity(BUFFER_SIZE, file),
            output: &mut self.inner,
            since_check: BUFFER_SIZE,
        };
        IsoImageWriter::format_new(&mut image, input, options).map_err(|err| match err {
            hadris_iso::write::IsoCreationError::Io(err) => {
                GeeZipError::io(err, "formatting ISO image")
            }
        })?;
        image.flush()?;
        let bytes = image.inner.get_ref().metadata()?.len();
        image.seek(SeekFrom::Start(0))?;
        // Release the output borrow before the final sequential copy.
        let file = image
            .inner
            .into_inner()
            .map_err(|err| GeeZipError::io(err.into_error(), "flushing ISO image"))?;
        drop(source);
        let copied = io::copy(
            &mut BufReader::with_capacity(BUFFER_SIZE, file),
            &mut self.inner,
        )?;
        if copied != bytes {
            return Err(GeeZipError::format(
                "ISO image temporary file changed during output",
                ArchiveFormat::Iso,
            ));
        }
        self.inner.flush()?;
        Ok((copied, self.inner))
    }
}

/// Buffer tiny metadata writes. Probe the original output's flush at most one
/// payload chunk apart, preserving ProgressWriter cancellation during formatting.
struct ImageSpool<W: Write> {
    inner: BufWriter<File>,
    output: W,
    since_check: usize,
}
impl<W: Write> Read for ImageSpool<W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.flush()?;
        self.inner.get_mut().read(buf)
    }
}
impl<W: Write> Write for ImageSpool<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.since_check >= BUFFER_SIZE {
            self.output.flush()?;
            self.since_check = 0;
        }
        let count = self.inner.write(buf)?;
        self.since_check += count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()?;
        self.inner.flush()
    }
}
impl<W: Write> Seek for ImageSpool<W> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

fn spool_entry(file: &mut File, reader: &mut dyn Read, limit: u64) -> GeeZipResult<u64> {
    let mut output = BufWriter::with_capacity(BUFFER_SIZE, file);
    let mut buf = [0u8; BUFFER_SIZE];
    let mut size = 0;
    loop {
        let max = (limit + 1 - size).min(BUFFER_SIZE as u64) as usize;
        let n = match reader.read(&mut buf[..max]) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > limit {
            return Err(GeeZipError::format(
                "ISO entry exceeds size limit",
                ArchiveFormat::Iso,
            ));
        }
        output.write_all(&buf[..n])?;
    }
    output.flush()?;
    Ok(size)
}

impl<W: Write + Send> ArchiveWriter for IsoWriter<W> {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Iso
    }
    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let path = self.check_path(path, false)?;
        if self.input.is_none() {
            self.input = Some(
                tempfile::tempfile()
                    .map_err(|e| GeeZipError::io(e, "creating ISO input temporary file"))?,
            );
        }
        let input = self.input.as_mut().expect("input initialized");
        let offset = input.seek(SeekFrom::End(0))?;
        let len = match spool_entry(input, reader, MAX_ENTRY_SIZE) {
            Ok(len) => len,
            Err(err) => {
                // The BufWriter has dropped; rollback includes any partial flush.
                if input
                    .set_len(offset)
                    .and_then(|_| input.seek(SeekFrom::Start(offset)))
                    .is_err()
                {
                    self.failed = true;
                }
                return Err(err);
            }
        };
        self.register(path, StagedEntry::File { offset, len });
        Ok(())
    }
    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let path = self.check_path(path, true)?;
        self.register(path, StagedEntry::Directory);
        Ok(())
    }
    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        (*self).finalize().map(|(bytes, _)| bytes)
    }
}
