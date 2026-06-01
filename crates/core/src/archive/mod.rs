//! Archive format abstraction layer.
//!
//! Defines the shared traits [`ArchiveReader`] and [`ArchiveWriter`] that
//! every archive format implementation must satisfy.  Concrete
//! implementations live in submodules (`zip`, `tar`, ...).
//!
//! # Design goals
//!
//! - **Core/CLI decoupling** — callers interact only via these traits and
//!   the [`Entry`] / [`ExtractReport`] types.
//! - **Streaming** — individual entry payloads flow through [`Read`] /
//!   [`Write`] so large files never need to be fully buffered.
//! - **Object-safe** — both traits use `&mut self` / `self: Box<Self>`
//!   receivers so they can be used through `Box<dyn ArchiveReader>` etc.

pub mod gzip;
pub mod tar;
pub mod targz;
pub mod zip;

use std::io::{Read, Write};
use std::path::Path;

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

/// A single entry inside an archive.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Relative path inside the archive (forward-slash separated).
    pub path: String,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// Compressed size in bytes (0 if unknown).
    pub compressed_size: u64,
    /// CRC-32 checksum, if the format provides it.
    pub crc32: Option<u32>,
    /// Last modification time as Unix timestamp (seconds since epoch), if available.
    pub modified: Option<u64>,
}

/// Convert a calendar date/time to a Unix timestamp (seconds since epoch).
///
/// Used by archive readers that provide modification timestamps in a
/// decomposed form that we cannot assume the OS can `mktime` for us.
/// This avoids pulling in a date-time crate for the core library.
pub(crate) fn datetime_to_timestamp(
    year: u64,
    month: u64,
    day: u64,
    hour: u64,
    minute: u64,
    second: u64,
) -> u64 {
    // Validate month/day to prevent silent garbage results (e.g. from malformed
    // archive metadata). Returns 0 on invalid input as a safe fallback.
    if !(1..=12).contains(&month) || day == 0 {
        return 0;
    }
    let is_leap = |y: u64| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let days_in_months: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    // Also reject day exceeding the month's length.
    let max_day = if month == 2 && is_leap(year) {
        29
    } else {
        days_in_months[(month - 1) as usize]
    };
    if day > max_day {
        return 0;
    }

    // Days from 1970-01-01 to (year-01-01).
    let mut total_days = 0u64;
    for y in 1970..year {
        total_days += if is_leap(y) { 366 } else { 365 };
    }

    // Days from (year-01-01) to (year-month-day).
    for m in 0..(month.saturating_sub(1)) {
        let idx = m as usize;
        total_days += if idx == 1 && is_leap(year) {
            29
        } else {
            days_in_months[idx]
        };
    }
    total_days += day.saturating_sub(1);

    total_days * 86_400 + hour * 3_600 + minute * 60 + second
}
/// Result of extracting an entire archive.
#[derive(Debug, Default)]
pub struct ExtractReport {
    /// Number of files successfully extracted.
    pub files_extracted: usize,
    /// Total uncompressed bytes written.
    pub bytes_extracted: u64,
    /// Number of files skipped (e.g. due to `--no-clobber`).
    pub files_skipped: usize,
    /// Per-file errors that did **not** abort the whole operation.
    pub errors: Vec<(String, crate::error::GeeZipError)>,
}

/// Archive reader trait — list entries and extract data.
///
/// Implementations are expected to be object-safe so callers can work
/// with `Box<dyn ArchiveReader>`.
pub trait ArchiveReader: Send {
    /// The format of the archive being read.
    fn format(&self) -> ArchiveFormat;

    /// Return the list of entries in the archive.
    fn entries(&mut self) -> GeeZipResult<Vec<Entry>>;

    /// Extract a single entry's content into `writer`.
    ///
    /// The caller is responsible for locating the output path; this
    /// method only writes the decompressed payload.
    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64>;

    /// Extract all entries under `dest`.
    ///
    /// Default implementation calls [`extract`](ArchiveReader::extract)
    /// for each entry, creating parent directories as needed.
    fn extract_all(&mut self, dest: &Path, overwrite: bool) -> GeeZipResult<ExtractReport> {
        let entries = self.entries()?;
        let mut report = ExtractReport::default();

        // Normalise the destination once so we use a consistent base.
        let dest = normalize_path(dest);

        for entry in &entries {
            let entry_path = Path::new(&entry.path);

            // --- Path safety checks (Zip Slip protection) ---
            let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                Ok(t) => t,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    continue;
                }
            };

            // Create parent directory.
            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::io(e, "creating parent directory"),
                        ));
                        continue;
                    }
                }
            }

            // Write entry content — atomically create or fail (avoids TOCTOU
            // between path-exists check and file creation for the no-clobber path).
            let mut output = if overwrite {
                match std::fs::File::create(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::io(
                                e,
                                format!("creating output file '{}'", target.display()),
                            ),
                        ));
                        continue;
                    }
                }
            } else {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        report.files_skipped += 1;
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::clobber_denied(target.display().to_string()),
                        ));
                        continue;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::io(
                                e,
                                format!("creating output file '{}'", target.display()),
                            ),
                        ));
                        continue;
                    }
                }
            };

            match self.extract(entry, &mut output) {
                Ok(bytes) => {
                    report.files_extracted += 1;
                    report.bytes_extracted += bytes;
                }
                Err(e) => {
                    report.errors.push((entry.path.clone(), e));
                }
            }
        }

        Ok(report)
    }

    /// Extract all entries under `dest`, checking `is_cancelled()` before
    /// each entry and before every write to the output file.
    ///
    /// The default implementation mirrors [`extract_all`](ArchiveReader::extract_all)
    /// but also wraps the output file in a `CancellableWriter` that returns
    /// `ErrorKind::Interrupted` when the user presses Ctrl+C.
    ///
    /// When cancellation is detected the operation is aborted immediately and
    /// returns [`GeeZipError::Cancelled`]. Already-extracted files are
    /// preserved (the caller decides whether to report the partial state).
    fn extract_all_with_cancel(
        &mut self,
        dest: &Path,
        overwrite: bool,
        is_cancelled: &dyn Fn() -> bool,
    ) -> GeeZipResult<ExtractReport> {
        let entries = self.entries()?;
        let mut report = ExtractReport::default();

        // Normalise the destination once so we use a consistent base.
        let dest = normalize_path(dest);

        for entry in &entries {
            // Check cancellation before processing each entry.
            if is_cancelled() {
                return Err(GeeZipError::Cancelled);
            }

            let entry_path = Path::new(&entry.path);

            // --- Path safety checks (Zip Slip protection) ---
            let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                Ok(t) => t,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    continue;
                }
            };

            // Create parent directory.
            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::io(e, "creating parent directory"),
                        ));
                        continue;
                    }
                }
            }

            // Write entry content — atomically create or fail.
            let output = if overwrite {
                match std::fs::File::create(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::io(
                                e,
                                format!("creating output file '{}'", target.display()),
                            ),
                        ));
                        continue;
                    }
                }
            } else {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        report.files_skipped += 1;
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::clobber_denied(target.display().to_string()),
                        ));
                        continue;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            crate::error::GeeZipError::io(
                                e,
                                format!("creating output file '{}'", target.display()),
                            ),
                        ));
                        continue;
                    }
                }
            };

            let mut output = CancellableWriter::new(output, is_cancelled);

            match self.extract(entry, &mut output) {
                Ok(bytes) => {
                    if output.was_cancelled() {
                        return Err(GeeZipError::Cancelled);
                    }
                    report.files_extracted += 1;
                    report.bytes_extracted += bytes;
                }
                Err(e) => {
                    report.errors.push((entry.path.clone(), e));
                }
            }
        }

        Ok(report)
    }
}

/// Archive writer trait — add files and finalise.
///
/// Implementations own their output writer internally.  Callers construct
/// the writer then call [`finish`](ArchiveWriter::finish) to finalise the
/// archive.
pub trait ArchiveWriter: Send {
    /// The format being written.
    fn format(&self) -> ArchiveFormat;

    /// Add a new entry from a byte stream.
    ///
    /// **Note:** Some archive formats (notably tar) require the entry
    /// size to be known _before_ the payload is written, which means
    /// the implementation may need to buffer the entire entry in
    /// memory.  For streaming-friendly formats (e.g., ZIP) this is
    /// handled transparently by the encoder.  Callers should avoid
    /// passing unbounded streams when using tar-based writers.
    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()>;

    /// Finalise the archive and return the total bytes written.
    ///
    /// After this call the writer is consumed and must not be used again.
    fn finish(self: Box<Self>) -> GeeZipResult<u64>;
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve a path by eliminating `.` and `..` components, without
/// accessing the filesystem (unlike `canonicalize`).
pub(crate) fn normalize_path(path: &Path) -> std::path::PathBuf {
    let mut components: Vec<std::ffi::OsString> = Vec::new();
    let root_separator = std::ffi::OsString::from(std::path::MAIN_SEPARATOR.to_string());

    for component in path.components() {
        match component {
            std::path::Component::RootDir => {
                components.push(root_separator.clone());
            }
            std::path::Component::CurDir => {
                // Keep the first CurDir so that normalize_path(".") stays "."
                // and normalize_path("./foo") stays "./foo".
                // This preserves the relative prefix for starts_with
                // comparisons in extract_all's Zip Slip check.
                if components.is_empty() {
                    components.push(std::ffi::OsString::from("."));
                }
            }
            std::path::Component::ParentDir => {
                if let Some(last) = components.last() {
                    if last.as_os_str() == root_separator.as_os_str() {
                        // Cannot go above root (e.g. /foo/.. stays /) — discard.
                        continue;
                    } else if last.as_os_str() == "." {
                        // `.` followed by `..` → replace with `..`
                        components.pop();
                        components.push(std::ffi::OsString::from(".."));
                    } else if last.as_os_str() == ".." {
                        // Multiple `..` in a row — keep them all
                        components.push(std::ffi::OsString::from(".."));
                    } else {
                        // Normal component — pop it (foo/.. cancels out)
                        components.pop();
                    }
                } else {
                    // Leading `..` with nothing to pop — keep it
                    components.push(std::ffi::OsString::from(".."));
                }
            }
            c => components.push(c.as_os_str().to_os_string()),
        }
    }

    let mut result = std::path::PathBuf::new();
    for c in components {
        result.push(c);
    }
    // If the result would be empty (e.g. input was `foo/..`), emit `.`
    // so that `starts_with` checks still work correctly.
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    result
}

/// Check whether extracting `entry_path` under `dest` would escape the
/// target directory (Zip Slip protection).
///
/// Returns `Ok(normalised_target)` if safe, or `Err((name, err))` suitable
/// for pushing into [`ExtractReport::errors`].
pub(crate) fn check_entry_path_safety(
    entry_path: &Path,
    entry_name: &str,
    dest: &Path,
) -> std::result::Result<std::path::PathBuf, (String, GeeZipError)> {
    // Reject absolute entry paths (e.g. /etc/passwd, C:\\foo).
    if entry_path.has_root() {
        return Err((
            entry_name.to_owned(),
            GeeZipError::PathTraversal {
                entry: entry_name.to_owned(),
                target: dest.display().to_string(),
            },
        ));
    }

    // On Windows, also reject UNC / device-prefixed paths.
    #[cfg(windows)]
    {
        let path_os = entry_name.replace("/", "\\");
        if path_os.starts_with("\\\\") {
            return Err((
                entry_name.to_owned(),
                GeeZipError::PathTraversal {
                    entry: entry_name.to_owned(),
                    target: dest.display().to_string(),
                },
            ));
        }
    }

    // Resolve the target relative to dest, then normalise.
    let target = normalize_path(&dest.join(entry_path));

    // Verify the resolved path is still under dest.
    if !target.starts_with(dest) {
        return Err((
            entry_name.to_owned(),
            GeeZipError::PathTraversal {
                entry: entry_name.to_owned(),
                target: dest.display().to_string(),
            },
        ));
    }

    Ok(target)
}

/// Counting writer wrapper that tracks total bytes written through a
/// writer chain.
///
/// Used internally by [`TarWriter`](super::tar::TarWriter) and
/// [`TarGzWriter`](super::targz::TarGzWriter).
pub(crate) struct CountWriter<W> {
    pub(crate) inner: W,
    pub(crate) count: u64,
}

impl<W: std::io::Write> std::io::Write for CountWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

// ---------------------------------------------------------------------------
// CancellableWriter
// ---------------------------------------------------------------------------

/// Writer wrapper that checks cancellation before every `write` and `flush`.
///
/// Used by [`extract_all_with_cancel`](ArchiveReader::extract_all_with_cancel)
/// to support Ctrl+C cancellation during per-entry extraction.
///
/// On cancellation the wrapper returns `ErrorKind::Interrupted` with
/// message "operation cancelled by user".
pub(crate) struct CancellableWriter<'a, W> {
    inner: W,
    is_cancelled: &'a dyn Fn() -> bool,
    cancelled: bool,
}

impl<'a, W> CancellableWriter<'a, W> {
    pub(crate) fn new(inner: W, is_cancelled: &'a dyn Fn() -> bool) -> Self {
        CancellableWriter {
            inner,
            is_cancelled,
            cancelled: false,
        }
    }

    /// Returns `true` if a cancellation was detected during the last write.
    pub(crate) fn was_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl<W: std::io::Write> std::io::Write for CancellableWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if (self.is_cancelled)() {
            self.cancelled = true;
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "operation cancelled by user",
            ));
        }
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if (self.is_cancelled)() {
            self.cancelled = true;
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "operation cancelled by user",
            ));
        }
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_simple() {
        let p = normalize_path(Path::new("/a/b/c"));
        assert_eq!(p, Path::new("/a/b/c"));
    }

    #[test]
    fn normalize_path_with_dotdot() {
        let p = normalize_path(Path::new("/a/b/../c"));
        assert_eq!(p, Path::new("/a/c"));
    }

    #[test]
    fn normalize_path_with_curdir() {
        let p = normalize_path(Path::new("/a/./b"));
        assert_eq!(p, Path::new("/a/b"));
    }

    #[test]
    fn normalize_path_below_root_escape() {
        // Escaping above root — this is fine, normalize just does its thing.
        let p = normalize_path(Path::new("/a/../../c"));
        assert_eq!(p, Path::new("/c"));
    }

    #[test]
    fn normalize_path_preserves_leading_curdir() {
        // normalize_path must keep a leading CurDir so that
        // starts_with comparisons work for dest=".".
        assert_eq!(normalize_path(Path::new(".")), Path::new("."));
        assert_eq!(normalize_path(Path::new("./foo")), Path::new("./foo"));
        assert_eq!(normalize_path(Path::new("./a/b")), Path::new("./a/b"));
    }

    #[test]
    fn normalize_path_with_leading_dotdot() {
        // Parent-dir at the start must be preserved.
        assert_eq!(normalize_path(Path::new("../foo")), Path::new("../foo"));
        assert_eq!(normalize_path(Path::new("./../foo")), Path::new("../foo"));
        assert_eq!(normalize_path(Path::new("./..")), Path::new(".."));
        assert_eq!(normalize_path(Path::new("./a/../../b")), Path::new("../b"));
    }

    #[test]
    fn normalize_path_multiple_dotdot() {
        assert_eq!(
            normalize_path(Path::new("../../foo")),
            Path::new("../../foo")
        );
        assert_eq!(
            normalize_path(Path::new("./../../foo")),
            Path::new("../../foo")
        );
        assert_eq!(normalize_path(Path::new("a/../..")), Path::new(".."));
    }

    #[test]
    fn normalize_path_complex_traversal() {
        assert_eq!(
            normalize_path(Path::new("a/b/../../../c")),
            Path::new("../c")
        );
        assert_eq!(normalize_path(Path::new("a/./../../b")), Path::new("../b"));
        assert_eq!(
            normalize_path(Path::new("a/./../b/.././c/../d")),
            Path::new("d")
        );
    }

    #[test]
    fn normalize_path_ipv6_root() {
        // Regression: check that OS root separator comparison works.
        assert_eq!(normalize_path(Path::new("/a/b/../c")), Path::new("/a/c"));
        assert_eq!(normalize_path(Path::new("/a/../../c")), Path::new("/c"));
    }

    #[test]
    fn normalize_path_curdir_peers() {
        // Ensure these still pass for dest="." checks.
        assert_eq!(normalize_path(Path::new(".")), Path::new("."));
        assert_eq!(normalize_path(Path::new("./foo")), Path::new("./foo"));
        assert_eq!(normalize_path(Path::new("./a/b")), Path::new("./a/b"));
    }

    #[test]
    fn cancellable_writer_detects_cancellation() {
        let mut buf = Vec::new();
        let cancelled = true;
        let is_cancelled = || cancelled;
        let mut writer = CancellableWriter::new(&mut buf, &is_cancelled);

        // write should fail with Interrupted
        let result = writer.write(b"hello");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert!(writer.was_cancelled());
        // buffer should be empty since the write didn't go through
        assert!(buf.is_empty());
    }

    #[test]
    fn cancellable_writer_passes_through_when_not_cancelled() {
        let mut buf = Vec::new();
        let cancelled = false;
        let is_cancelled = || cancelled;
        let mut writer = CancellableWriter::new(&mut buf, &is_cancelled);

        let n = writer.write(b"hello").unwrap();
        assert_eq!(n, 5);
        assert!(!writer.was_cancelled());
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn datetime_to_timestamp_rejects_invalid_month_day() {
        // month must be 1..=12, day must be >=1 and <= month length.
        assert_eq!(datetime_to_timestamp(2026, 0, 15, 0, 0, 0), 0);
        assert_eq!(datetime_to_timestamp(2026, 13, 15, 0, 0, 0), 0);
        assert_eq!(datetime_to_timestamp(2026, 6, 0, 0, 0, 0), 0);
        assert_eq!(datetime_to_timestamp(2026, 6, 31, 0, 0, 0), 0); // Jun has 30 days
        assert!(datetime_to_timestamp(2026, 6, 15, 0, 0, 0) > 0); // valid input => non-zero
        assert!(datetime_to_timestamp(2026, 6, 15, 0, 0, 0) > 0);
    }

    #[test]
    fn cancellable_writer_flush_detects_cancellation() {
        use std::cell::Cell;
        let cancelled = Cell::new(false);
        let is_cancelled = || cancelled.get();
        let mut buf = Vec::new();
        let mut writer = CancellableWriter::new(&mut buf, &is_cancelled);
        writer.write_all(b"data").unwrap();
        // Now flip the flag and flush should fail.
        cancelled.set(true);
        let result = writer.flush();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert!(writer.was_cancelled());
        // Data written before cancellation should still be present.
        assert_eq!(&buf, b"data");
    }
}
