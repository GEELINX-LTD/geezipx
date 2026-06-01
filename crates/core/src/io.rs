//! Streaming I/O wrappers with progress tracking.
//!
//! This module provides [`ProgressReader`] and [`ProgressWriter`] wrappers
//! that count bytes transferred and optionally invoke a
//! [`ProgressCallback`] to report progress.  When no callback is attached
//! the overhead is a single `Option` check (branch predictor wins on it).
//!
//! # Example
//!
//! ```ignore
//! use std::io::{Read, Cursor};
//! use geezipx_core::{Phase, ProgressReader};
//!
//! let data = b"hello world";
//! let mut reader = ProgressReader::new(Cursor::new(data))
//!     .with_total(data.len() as u64);
//! let mut buf = Vec::new();
//! reader.read_to_end(&mut buf).unwrap();
//! assert_eq!(reader.bytes_read(), data.len() as u64);
//! ```

use std::fmt;
use std::io::{Read, Write};

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

/// Identifies the current phase of an operation.
///
/// Used by [`ProgressEvent`] to distinguish reading from writing (and
/// hashing, reserved for future use).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Data is being read / decompressed.
    Reading,
    /// Data is being written / compressed.
    Writing,
    /// A checksum or hash is being computed (reserved for future use).
    Hashing,
}

// ---------------------------------------------------------------------------
// ProgressEvent
// ---------------------------------------------------------------------------

/// A single progress notification emitted by a streaming wrapper.
///
/// Delivered via [`ProgressCallback::update`].
#[derive(Debug, Clone, Copy)]
pub struct ProgressEvent {
    /// Bytes processed so far.
    pub current: u64,
    /// Total bytes expected, if known.
    pub total: Option<u64>,
    /// Whether we are reading or writing.
    pub phase: Phase,
}

// ---------------------------------------------------------------------------
// ProgressCallback
// ---------------------------------------------------------------------------

/// Trait for objects that can receive progress events from a streaming
/// wrapper.
///
/// All methods have default implementations so the simplest callback only
/// needs to override [`update`](ProgressCallback::update).
pub trait ProgressCallback: Send {
    /// Called after every successful `read` / `write` to report progress.
    fn update(&mut self, _event: ProgressEvent) {}

    /// Called to check whether the user has requested cancellation.
    ///
    /// Returns `false` by default.  NOTE: This method is defined for
    /// forward compatibility but is NOT yet called by ProgressReader
    /// or ProgressWriter.  It will be wired in M3-3 (Ctrl+C
    /// cancellation).  Override this now if you want — it will take
    /// effect once wired.
    fn is_cancelled(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// ProgressReader
// ---------------------------------------------------------------------------

/// A `Read` wrapper that counts bytes transferred and optionally
/// reports progress via a callback.
///
/// When no callback is attached the only overhead is a single `Option`
/// check per `read` call.
///
/// # Builder pattern
///
/// ```ignore
/// use std::io::Cursor;
/// use geezipx_core::ProgressReader;
///
/// let reader = ProgressReader::new(Cursor::new(b"data"))
///     .with_total(4)
///     .with_callback(Box::new(MyCallback));
/// ```
pub struct ProgressReader<R: Read> {
    inner: R,
    bytes_read: u64,
    total: Option<u64>,
    callback: Option<Box<dyn ProgressCallback>>,
}

impl<R: Read> ProgressReader<R> {
    /// Create a new wrapper with no callback and no known total.
    pub fn new(inner: R) -> Self {
        ProgressReader {
            inner,
            bytes_read: 0,
            total: None,
            callback: None,
        }
    }

    /// Attach a known total size (sets `total` in emitted events).
    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }

    /// Attach a progress callback.
    pub fn with_callback(mut self, cb: Box<dyn ProgressCallback>) -> Self {
        self.callback = Some(cb);
        self
    }

    /// Total bytes read so far.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Consume the wrapper and return the inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n as u64;

        if n > 0 {
            if let Some(ref mut cb) = self.callback {
                cb.update(ProgressEvent {
                    current: self.bytes_read,
                    total: self.total,
                    phase: Phase::Reading,
                });
            }
        }

        Ok(n)
    }
}

impl<R: Read + fmt::Debug> fmt::Debug for ProgressReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressReader")
            .field("inner", &self.inner)
            .field("bytes_read", &self.bytes_read)
            .field("total", &self.total)
            .field("has_callback", &self.callback.is_some())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// ProgressWriter
// ---------------------------------------------------------------------------

/// A `Write` wrapper that counts bytes transferred and optionally
/// reports progress via a callback.
///
/// When no callback is attached the only overhead is a single `Option`
/// check per `write` call.
pub struct ProgressWriter<W: Write> {
    inner: W,
    bytes_written: u64,
    total: Option<u64>,
    callback: Option<Box<dyn ProgressCallback>>,
}

impl<W: Write> ProgressWriter<W> {
    /// Create a new wrapper with no callback and no known total.
    pub fn new(inner: W) -> Self {
        ProgressWriter {
            inner,
            bytes_written: 0,
            total: None,
            callback: None,
        }
    }

    /// Attach a known total size.
    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }

    /// Attach a progress callback.
    pub fn with_callback(mut self, cb: Box<dyn ProgressCallback>) -> Self {
        self.callback = Some(cb);
        self
    }

    /// Total bytes written so far.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Consume the wrapper and return the inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for ProgressWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes_written += n as u64;

        if n > 0 {
            if let Some(ref mut cb) = self.callback {
                cb.update(ProgressEvent {
                    current: self.bytes_written,
                    total: self.total,
                    phase: Phase::Writing,
                });
            }
        }

        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + fmt::Debug> fmt::Debug for ProgressWriter<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressWriter")
            .field("inner", &self.inner)
            .field("bytes_written", &self.bytes_written)
            .field("total", &self.total)
            .field("has_callback", &self.callback.is_some())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A simple callback that collects every event it receives.
    #[derive(Debug)]
    struct TestCallback {
        events: Vec<ProgressEvent>,
    }

    impl TestCallback {
        fn new() -> Self {
            TestCallback { events: Vec::new() }
        }
    }

    impl ProgressCallback for TestCallback {
        fn update(&mut self, event: ProgressEvent) {
            self.events.push(event);
        }
    }

    // ---------------------------------------------------------------
    // ProgressReader tests
    // ---------------------------------------------------------------

    #[test]
    fn progress_reader_counts_bytes() {
        let data = b"hello, world!";
        let mut reader = ProgressReader::new(Cursor::new(data));
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(reader.bytes_read(), data.len() as u64);
        assert_eq!(&buf, data);
    }

    #[test]
    fn progress_reader_no_callback() {
        let data = b"no callback here";
        let mut reader = ProgressReader::new(Cursor::new(data));
        let mut buf = [0u8; 64];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, data.len());
        assert_eq!(&buf[..n], data);
    }

    #[test]
    fn progress_reader_with_callback() {
        let data = b"callback test data";
        let cb = TestCallback::new();
        let mut reader = ProgressReader::new(Cursor::new(data)).with_callback(Box::new(cb));
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();

        // The callback was moved in, we need to retrieve it.  Since we
        // cannot get it back, we verify the data instead and test event
        // delivery in a separate test that borrows the callback.
        assert_eq!(&buf, data);
        assert_eq!(reader.bytes_read(), data.len() as u64);
    }

    #[test]
    fn progress_reader_with_total() {
        let data = b"known length";
        let total = data.len() as u64;
        let mut reader = ProgressReader::new(Cursor::new(data)).with_total(total);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(reader.bytes_read(), total);
    }

    #[test]
    fn progress_reader_partial_reads() {
        let data = b"partial reads test";
        let mut reader = ProgressReader::new(Cursor::new(data));
        let mut buf = [0u8; 4];

        assert_eq!(reader.read(&mut buf).unwrap(), 4);
        assert_eq!(reader.bytes_read(), 4);

        assert_eq!(reader.read(&mut buf).unwrap(), 4);
        assert_eq!(reader.bytes_read(), 8);

        // Read remaining
        let mut rest = Vec::new();
        reader.read_to_end(&mut rest).unwrap();
        assert_eq!(reader.bytes_read(), data.len() as u64);
    }

    #[test]
    fn progress_reader_empty_input() {
        let data = b"";
        let cb = TestCallback::new();
        let mut reader = ProgressReader::new(Cursor::new(data)).with_callback(Box::new(cb));
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(reader.bytes_read(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn progress_reader_into_inner() {
        let data = b"give it back";
        let inner = Cursor::new(data);
        let reader = ProgressReader::new(inner);
        let recovered = reader.into_inner();
        assert_eq!(recovered.into_inner(), data);
    }

    // ---------------------------------------------------------------
    // ProgressWriter tests
    // ---------------------------------------------------------------

    #[test]
    fn progress_writer_counts_bytes() {
        let data = b"writing test";
        let mut writer = ProgressWriter::new(Vec::new());
        writer.write_all(data).unwrap();
        assert_eq!(writer.bytes_written(), data.len() as u64);
        assert_eq!(&writer.into_inner(), data);
    }

    #[test]
    fn progress_writer_no_callback() {
        let data = b"no callback writer";
        let mut writer = ProgressWriter::new(Vec::new());
        writer.write_all(data).unwrap();
        writer.flush().unwrap();
        assert_eq!(writer.bytes_written(), data.len() as u64);
    }

    #[test]
    fn progress_writer_with_callback() {
        let data = b"writer callback test";
        let mut writer =
            ProgressWriter::new(Vec::new()).with_callback(Box::new(TestCallback::new()));
        writer.write_all(data).unwrap();
        assert_eq!(writer.bytes_written(), data.len() as u64);
    }

    #[test]
    fn progress_writer_flush_and_into_inner() {
        let mut writer = ProgressWriter::new(Vec::new());
        writer.write_all(b"flush test").unwrap();
        writer.flush().unwrap();
        let inner = writer.into_inner();
        assert_eq!(&inner, b"flush test");
    }

    // ---------------------------------------------------------------
    // Event verification — test that callbacks actually receive events
    // ---------------------------------------------------------------

    #[test]
    fn progress_reader_events_are_correct() {
        use std::sync::{Arc, Mutex};

        let data = b"event check";
        let cb = Arc::new(Mutex::new(TestCallback::new()));

        struct EventCollector(Arc<Mutex<TestCallback>>);
        impl ProgressCallback for EventCollector {
            fn update(&mut self, event: ProgressEvent) {
                self.0.lock().unwrap().events.push(event);
            }
        }

        let mut reader = ProgressReader::new(Cursor::new(data))
            .with_total(data.len() as u64)
            .with_callback(Box::new(EventCollector(cb.clone())));

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();

        let events = cb.lock().unwrap().events.clone();
        assert!(!events.is_empty(), "should have received events");

        let last = events.last().unwrap();
        assert_eq!(last.current, data.len() as u64);
        assert_eq!(last.total, Some(data.len() as u64));
        assert_eq!(last.phase, Phase::Reading);
    }

    #[test]
    fn progress_writer_events_are_correct() {
        use std::sync::{Arc, Mutex};

        let data = b"writer events";
        let cb = Arc::new(Mutex::new(TestCallback::new()));

        struct EventCollector(Arc<Mutex<TestCallback>>);
        impl ProgressCallback for EventCollector {
            fn update(&mut self, event: ProgressEvent) {
                self.0.lock().unwrap().events.push(event);
            }
        }

        let mut writer = ProgressWriter::new(Vec::new())
            .with_total(data.len() as u64)
            .with_callback(Box::new(EventCollector(cb.clone())));

        writer.write_all(data).unwrap();

        let events = cb.lock().unwrap().events.clone();
        assert!(!events.is_empty(), "should have received events");

        let last = events.last().unwrap();
        assert_eq!(last.current, data.len() as u64);
        assert_eq!(last.total, Some(data.len() as u64));
        assert_eq!(last.phase, Phase::Writing);
    }

    // ---------------------------------------------------------------
    // Derive / trait checks
    // ---------------------------------------------------------------

    #[test]
    fn progress_event_debug_clone() {
        let event = ProgressEvent {
            current: 42,
            total: Some(100),
            phase: Phase::Reading,
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("42"));
        assert!(debug.contains("100"));
        assert!(debug.contains("Reading"));
    }

    #[test]
    fn progress_callback_default_is_cancelled() {
        struct Dummy;
        impl ProgressCallback for Dummy {}

        let cb = Dummy;
        assert!(!cb.is_cancelled());
    }

    #[test]
    fn phase_debug_clone_partial_eq() {
        assert_eq!(Phase::Reading, Phase::Reading);
        assert_ne!(Phase::Reading, Phase::Writing);
        assert_ne!(Phase::Hashing, Phase::Writing);
        let _ = format!("{:?}", Phase::Hashing);
    }

    // ---------------------------------------------------------------
    // Gzip roundtrip with progress
    // ---------------------------------------------------------------

    #[test]
    fn gzip_roundtrip_with_progress() {
        use crate::archive::gzip;
        use std::sync::{Arc, Mutex};

        let original = b"gzip progress roundtrip test data!";

        // Count reader events.
        let read_events = Arc::new(Mutex::new(TestCallback::new()));
        struct ReadCollector(Arc<Mutex<TestCallback>>);
        impl ProgressCallback for ReadCollector {
            fn update(&mut self, event: ProgressEvent) {
                self.0.lock().unwrap().events.push(event);
            }
        }

        // Compress with progress reader.
        let mut source = ProgressReader::new(Cursor::new(original))
            .with_total(original.len() as u64)
            .with_callback(Box::new(ReadCollector(read_events.clone())));

        let compressed = {
            let mut buf = Vec::new();
            gzip::gzip_compress(&mut source, &mut buf).unwrap();
            buf
        };

        // Verify reader events.
        {
            let events = read_events.lock().unwrap().events.clone();
            assert!(!events.is_empty(), "reader should have received events");
            let last = events.last().unwrap();
            assert_eq!(last.current, original.len() as u64);
            assert_eq!(last.total, Some(original.len() as u64));
            assert_eq!(last.phase, Phase::Reading);
        }

        // Count writer events.
        let write_events = Arc::new(Mutex::new(TestCallback::new()));
        struct WriteCollector(Arc<Mutex<TestCallback>>);
        impl ProgressCallback for WriteCollector {
            fn update(&mut self, event: ProgressEvent) {
                self.0.lock().unwrap().events.push(event);
            }
        }

        // Decompress with progress writer.
        let mut decompress_target = ProgressWriter::new(Vec::new())
            .with_callback(Box::new(WriteCollector(write_events.clone())));

        let mut compressed_reader = Cursor::new(compressed.as_slice());
        let bytes = gzip::gzip_decompress(&mut compressed_reader, &mut decompress_target).unwrap();

        // Verify content matches.
        let decompressed = decompress_target.into_inner();
        assert_eq!(bytes, original.len() as u64);
        assert_eq!(decompressed, original);

        // Verify writer events.
        {
            let events = write_events.lock().unwrap().events.clone();
            assert!(!events.is_empty(), "writer should have received events");
            let last = events.last().unwrap();
            assert_eq!(last.current, original.len() as u64);
            assert_eq!(last.phase, Phase::Writing);
            // total is None because we didn't set it on the writer
            assert_eq!(last.total, None);
        }
    }

    // ---------------------------------------------------------------
    // Composition tests
    // ---------------------------------------------------------------

    #[test]
    fn progress_reader_bufreader_composition() {
        let data = vec![0u8; 10_000];
        let pr = ProgressReader::new(std::io::Cursor::new(data.clone())).with_total(10_000);
        let mut br = std::io::BufReader::new(pr);
        let mut buf = Vec::new();
        br.read_to_end(&mut buf).unwrap();
        assert_eq!(br.into_inner().bytes_read(), 10_000);
        assert_eq!(buf.len(), 10_000);
    }

    #[test]
    fn progress_writer_bufwriter_composition() {
        let data = vec![0xABu8; 10_000];
        let pw = ProgressWriter::new(Vec::new()).with_total(10_000);
        let mut bw = std::io::BufWriter::new(pw);
        bw.write_all(&data).unwrap();
        bw.flush().unwrap();
        let inner = bw.into_inner().unwrap();
        assert_eq!(inner.bytes_written(), 10_000);
        assert_eq!(inner.into_inner(), data);
    }

    // ---------------------------------------------------------------
    // Error propagation tests
    // ---------------------------------------------------------------

    struct FailingReader {
        reads_before_fail: usize,
        read_count: usize,
    }

    impl std::io::Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.read_count >= self.reads_before_fail {
                return Err(std::io::Error::other("test error"));
            }
            self.read_count += 1;
            if !buf.is_empty() {
                buf[0] = 0x01;
                Ok(1)
            } else {
                Ok(0)
            }
        }
    }

    #[test]
    fn progress_reader_passes_errors() {
        let failing = FailingReader {
            reads_before_fail: 2,
            read_count: 0,
        };
        let mut pr = ProgressReader::new(failing);
        let mut buf = [0u8; 16];
        assert!(pr.read(&mut buf).is_ok());
        assert!(pr.read(&mut buf).is_ok());
        let err = pr.read(&mut buf).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
        assert_eq!(err.to_string(), "test error");
    }

    struct FailingWriter {
        writes_before_fail: usize,
        write_count: usize,
    }

    impl std::io::Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.write_count >= self.writes_before_fail {
                return Err(std::io::Error::other("test error"));
            }
            self.write_count += 1;
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn progress_writer_passes_errors() {
        let failing = FailingWriter {
            writes_before_fail: 2,
            write_count: 0,
        };
        let mut pw = ProgressWriter::new(failing);
        assert!(pw.write(b"aaa").is_ok());
        assert!(pw.write(b"bbb").is_ok());
        let err = pw.write(b"ccc").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
        assert_eq!(err.to_string(), "test error");
    }
}
