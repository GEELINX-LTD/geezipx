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
    fn extract_all(&mut self, dest: &Path) -> GeeZipResult<ExtractReport> {
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

            // Write entry content.
            let mut output = match std::fs::File::create(&target) {
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
}
