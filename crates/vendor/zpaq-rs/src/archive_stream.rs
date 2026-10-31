//! Multi-file streaming archives using one solid compression block.
use super::{FfiReader, FfiWriter, Result, ZpaqError, err_from_last, start_block_for_method, sys};
use std::ffi::CString;
use std::io::{self, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Mutex;

/// One named segment. `size` is the exact uncompressed byte count.
pub struct ReaderArchiveEntry<R> {
    pub path: String,
    pub reader: R,
    pub size: u64,
    pub comment: Option<String>,
}

// Raw C++ objects are confined here. All callback contexts remain alive while
// their pointers are used; the compressor does not own its input/output handles.
struct Compressor(*mut sys::Compressor);
impl Drop for Compressor {
    fn drop(&mut self) {
        // SAFETY: allocated once by zpaq_compressor_new, uniquely owned here.
        unsafe { sys::zpaq_compressor_free(self.0) };
    }
}

struct GuardedIo<T>(T);
impl<T: Read> Read for GuardedIo<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len();
        let n = catch_unwind(AssertUnwindSafe(|| self.0.read(buf)))
            .map_err(|_| io::Error::other("input callback panicked"))??;
        if n > len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid reader count",
            ));
        }
        Ok(n)
    }
}
impl<T: Write> Write for GuardedIo<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        catch_unwind(AssertUnwindSafe(|| self.0.write(buf)))
            .map_err(|_| io::Error::other("output callback panicked"))?
    }
    fn flush(&mut self) -> io::Result<()> {
        catch_unwind(AssertUnwindSafe(|| self.0.flush()))
            .map_err(|_| io::Error::other("output flush callback panicked"))?
    }
}
struct LockedOutput<'a, W>(&'a Mutex<GuardedIo<W>>);
impl<W: Write> Write for LockedOutput<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("output lock poisoned"))?
            .write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("output lock poisoned"))?
            .flush()
    }
}
fn check(rc: i32) -> Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(err_from_last())
    }
}
fn io_error(err: io::Error) -> ZpaqError {
    ZpaqError::Ffi(err.to_string())
}

/// Encode named readers into one solid block without retaining their payloads.
///
/// Input/output errors are returned, including the final C++ buffer flush.
/// Readers may return short reads or Interrupted. A zero-length output write is
/// issued at each 64 KiB step so cancellation remains observable even when the
/// compressed stream has not filled the C++ output buffer. Callers should write
/// to a temporary destination: errors can leave a partial archive prefix.
/// Numeric methods 1–3 use libzpaq's built-in streaming models; model memory is
/// independent of the total payload length. Preprocessing methods are rejected.
pub fn archive_from_reader_entries<I, R, W>(entries: I, writer: W, method: &str) -> Result<()>
where
    I: IntoIterator<Item = ReaderArchiveEntry<R>>,
    R: Read + Send,
    W: Write + Send,
{
    let mut entries = entries.into_iter().peekable();
    if entries.peek().is_none() {
        return GuardedIo(writer).flush().map_err(io_error);
    }
    let shared = Mutex::new(GuardedIo(writer));
    let mut polling_output = LockedOutput(&shared);
    polling_output.write(&[]).map_err(io_error)?;
    let output = FfiWriter::new(LockedOutput(&shared))?;
    // SAFETY: C++ constructor returns an owned opaque handle; output remains
    // alive until after this guard is destroyed, including on every error path.
    let compressor = Compressor(unsafe { sys::zpaq_compressor_new() });
    if compressor.0.is_null() {
        return Err(err_from_last());
    }
    unsafe {
        check(sys::zpaq_compressor_set_output(compressor.0, output.raw))?;
        check(sys::zpaq_compressor_write_tag(compressor.0))?;
    }
    start_block_for_method(compressor.0, method)?;
    unsafe {
        check(sys::zpaq_compressor_set_verify(compressor.0, 1))?;
    }
    for entry in entries {
        if entry.path.is_empty() || entry.size > i64::MAX as u64 {
            return Err(ZpaqError::Ffi("invalid stream entry name or size".into()));
        }
        let name = CString::new(entry.path).map_err(|_| ZpaqError::NulInString)?;
        let comment = CString::new(entry.comment.unwrap_or_else(|| entry.size.to_string()))
            .map_err(|_| ZpaqError::NulInString)?;
        // Bound dishonest input sizes as well as memory; one extra byte detects
        // a reader longer than declared without consuming an unbounded stream.
        let input = FfiReader::new(GuardedIo(entry.reader.take(entry.size + 1)))?;
        unsafe {
            check(sys::zpaq_compressor_set_input(compressor.0, input.raw))?;
            check(sys::zpaq_compressor_start_segment(
                compressor.0,
                name.as_ptr(),
                comment.as_ptr(),
            ))?;
        }
        loop {
            polling_output.write(&[]).map_err(io_error)?;
            let rc = unsafe { sys::zpaq_compressor_compress(compressor.0, 64 * 1024) };
            match rc {
                0 => break,
                1 => (),
                _ => return Err(err_from_last()),
            }
        }
        let mut actual = 0i64;
        let rc = unsafe {
            sys::zpaq_compressor_end_segment_checksum(compressor.0, &mut actual, 1, ptr::null_mut())
        };
        if rc != 1 {
            return Err(err_from_last());
        }
        if actual < 0 || actual as u64 != entry.size {
            return Err(ZpaqError::Ffi(format!(
                "stream entry size mismatch: expected {}, got {actual}",
                entry.size
            )));
        }
        // C++ retains an input pointer, but neither endBlock nor destruction
        // reads it. The next segment replaces it before any further reads.
    }
    unsafe {
        check(sys::zpaq_compressor_end_block(compressor.0))?;
        check(sys::zpaq_writer_flush(output.raw))?;
    }
    polling_output.flush().map_err(io_error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
