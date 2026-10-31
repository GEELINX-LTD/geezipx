use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};

/// Poll the operation's process-wide flag while a codec consumes staged input.
/// Empty writes deliberately reach this wrapper, allowing CPU/block encoders
/// to poll without changing the archive contents.
pub(super) struct CancellableOutput(pub(super) File);

impl Write for CancellableOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if crate::signal::is_cancelled() {
            return Err(io::Error::other(geezipx_core::GeeZipError::Cancelled));
        }
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        if crate::signal::is_cancelled() {
            return Err(io::Error::other(geezipx_core::GeeZipError::Cancelled));
        }
        self.0.flush()
    }
}
impl Seek for CancellableOutput {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.0.seek(position)
    }
}
