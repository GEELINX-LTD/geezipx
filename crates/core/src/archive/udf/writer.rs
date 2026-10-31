//! UDF creation with payloads and the seekable image held on disk.
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};

use hadris_udf::write::{
    FileSource, SimpleDir, SimpleFile, UdfWriteOptions, UdfWriter as Formatter,
};
use hadris_udf::UdfError;

use crate::archive::ArchiveWriter;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

const BUFFER_SIZE: usize = 64 * 1024;

#[cfg(test)]
mod tests;

/// UDF writer accepting arbitrary Read inputs and non-seekable Write outputs.
///
/// Inputs are snapshotted into one anonymous temporary file. A second temporary
/// file holds the seekable image. Payload RAM is bounded; directory metadata
/// scales with entry count. Temporary disk space is about input plus image size.
/// Creation retains the existing 4 GiB per-file limit and UDF 1.02 revision.
pub struct UdfWriter<W: Write + Send> {
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

impl<W: Write + Send> std::fmt::Debug for UdfWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdfWriter")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl<W: Write + Send> UdfWriter<W> {
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
            return Err(format_error("UDF staging file unusable after I/O failure"));
        }
        let normalized = super::validate_udf_entry_path(path)?;
        let path = Path::new(&normalized)
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if path.is_empty() {
            return Err(format_error("invalid empty UDF entry path"));
        }
        if path.split('/').count() > 128 + usize::from(!is_dir) {
            return Err(format_error("UDF directory depth exceeds 128"));
        }
        for name in path.split('/') {
            let encoded = if name.is_ascii() {
                1 + name.len()
            } else {
                1 + 2 * name.encode_utf16().count()
            };
            if name.contains('\0') || encoded > u8::MAX as usize {
                return Err(format_error(
                    "UDF name exceeds 255 encoded bytes or contains NUL",
                ));
            }
        }
        if let Some(existing) = self.entries.get(&path) {
            if !is_dir || !matches!(existing, StagedEntry::Directory) {
                return Err(format_error(format!(
                    "duplicate or conflicting UDF entry: {path}"
                )));
            }
        }
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            if matches!(self.entries.get(prefix), Some(StagedEntry::File { .. })) {
                return Err(format_error(format!("UDF parent is a file: {prefix}")));
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

    /// Finish the exact image and recover the supplied output writer.
    pub fn finalize(mut self) -> GeeZipResult<(u64, W)> {
        if self.failed || self.entries.is_empty() {
            return Err(format_error(
                "cannot finalize an empty or failed UDF writer",
            ));
        }
        let source = self.input.take().map(|file| Arc::new(Mutex::new(file)));
        let mut children: BTreeMap<String, SimpleDir> = BTreeMap::new();
        // Children precede parents in reverse lexical order: no repeated tree
        // scans, recursive insertion, payload clones or per-entry file handles.
        for (path, entry) in self.entries.into_iter().rev() {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", &path));
            match entry {
                StagedEntry::Directory => {
                    let mut dir = children.remove(&path).unwrap_or_default();
                    dir.name = name.to_owned();
                    children.entry(parent.to_owned()).or_default().add_dir(dir);
                }
                StagedEntry::File { offset, len } => {
                    let data = FileSource::new(
                        source.as_ref().expect("staged input").clone(),
                        offset,
                        len,
                    )?;
                    children
                        .entry(parent.to_owned())
                        .or_default()
                        .add_file(SimpleFile::from_source(name, data));
                }
            }
        }
        let mut root = children.remove("").unwrap_or_default();
        root.sort();
        let options = UdfWriteOptions {
            volume_id: "GEEZIPX".into(),
            ..Default::default()
        };
        let file = tempfile::tempfile()
            .map_err(|e| GeeZipError::io(e, "creating UDF image temporary file"))?;
        let mut image = ImageSpool {
            inner: BufWriter::with_capacity(BUFFER_SIZE, file),
            output: &mut self.inner,
            since_check: BUFFER_SIZE,
        };
        let sectors = Formatter::format(&mut image, &root, options).map_err(|err| match err {
            UdfError::Io(err) => GeeZipError::io(err, "formatting UDF image"),
            err => format_error(format!("formatting UDF image: {err}")),
        })?;
        image.flush()?;
        let bytes = u64::from(sectors) * 2048;
        if image.inner.get_ref().metadata()?.len() > bytes {
            return Err(format_error(
                "UDF formatter wrote beyond declared image length",
            ));
        }
        // Seek-only gaps and the last partial descriptor must be readable zeros.
        image.inner.get_ref().set_len(bytes)?;
        image.seek(SeekFrom::Start(0))?;
        let file = image
            .inner
            .into_inner()
            .map_err(|e| GeeZipError::io(e.into_error(), "flushing UDF image"))?;
        drop(root);
        drop(source);
        let copied = io::copy(
            &mut BufReader::with_capacity(BUFFER_SIZE, file),
            &mut self.inner,
        )?;
        if copied != bytes {
            return Err(format_error(
                "UDF image temporary file changed during output",
            ));
        }
        self.inner.flush()?;
        Ok((copied, self.inner))
    }
}

fn format_error(message: impl Into<String>) -> GeeZipError {
    GeeZipError::format(message, ArchiveFormat::Udf)
}

/// Tiny descriptor writes are buffered; flush probes retain cancellation during
/// formatting even before any image bytes reach the original output.
struct ImageSpool<W: Write> {
    inner: BufWriter<File>,
    output: W,
    since_check: usize,
}
impl<W: Write> Write for ImageSpool<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.since_check >= BUFFER_SIZE {
            self.output.flush()?;
            self.since_check = 0;
        }
        let n = self.inner.write(buf)?;
        self.since_check += n;
        Ok(n)
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
            return Err(format_error("UDF entry exceeds size limit"));
        }
        output.write_all(&buf[..n])?;
    }
    output.flush()?;
    Ok(size)
}

impl<W: Write + Send> ArchiveWriter for UdfWriter<W> {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Udf
    }
    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let path = self.check_path(path, false)?;
        if self.input.is_none() {
            self.input = Some(
                tempfile::tempfile()
                    .map_err(|e| GeeZipError::io(e, "creating UDF input temporary file"))?,
            );
        }
        let input = self.input.as_mut().expect("input initialized");
        let offset = input.seek(SeekFrom::End(0))?;
        let len = match spool_entry(input, reader, super::UDF_MAX_ENTRY_SIZE) {
            Ok(len) => len,
            Err(err) => {
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
        (*self).finalize().map(|(n, _)| n)
    }
}
