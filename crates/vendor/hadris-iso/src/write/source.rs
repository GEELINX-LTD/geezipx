//! File-backed input without keeping file contents resident.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

/// An immutable byte range in a shared seekable file.
///
/// Cloning only clones the handle. Callers must keep the file contents stable
/// while formatting an image. The mutex serializes offsets across ranges and
/// is released before writing to the output (including asynchronous output).
#[derive(Clone, Debug)]
pub struct FileSource {
    file: Arc<Mutex<File>>,
    offset: u64,
    len: usize,
}

impl FileSource {
    pub fn new(file: Arc<Mutex<File>>, offset: u64, len: u64) -> io::Result<Self> {
        let end = offset.checked_add(len).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "file source range overflow")
        })?;
        let size = file
            .lock()
            .map_err(|_| io::Error::other("file source lock poisoned"))?
            .metadata()?
            .len();
        if end > size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "file source range exceeds file",
            ));
        }
        let len = usize::try_from(len).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "file source length exceeds usize",
            )
        })?;
        Ok(Self { file, offset, len })
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn read_at(&self, offset: usize, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.len.saturating_sub(offset).min(buf.len());
        if available == 0 {
            return Ok(0);
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("file source lock poisoned"))?;
        file.seek(SeekFrom::Start(self.offset + offset as u64))?;
        loop {
            match file.read(&mut buf[..available]) {
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

// Source equality is handle/range identity; it does not reread file contents.
impl PartialEq for FileSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.file, &other.file) && self.offset == other.offset && self.len == other.len
    }
}
impl Eq for FileSource {}
