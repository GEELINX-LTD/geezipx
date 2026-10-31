use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

/// Poll cancellation on empty reads too, including PEA's inflation loop.
pub(super) struct CancellableInput(pub(super) File);
impl Read for CancellableInput {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        check()?;
        self.0.read(bytes)
    }
}
impl Seek for CancellableInput {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        check()?;
        self.0.seek(position)
    }
}
fn check() -> io::Result<()> {
    if crate::signal::is_cancelled() {
        return Err(io::Error::other(geezipx_core::GeeZipError::Cancelled));
    }
    Ok(())
}
