//! Versioned binary action file protocol for shell context menu multi-select.
//!
//! ## Motivation
//!
//! Static shell verbs cannot reliably pass multiple selected paths to a
//! single application instance — `%*` expansion is fragile and often results
//! in the literal text `%*` or an empty argument list being passed to the
//! application.
//!
//! The reliable approach is a `DelegateExecute` COM handler that receives
//! `IShellItemArray` directly, then writes a *selection file* that the main
//! GUI process reads via `--shell-action-file`.  This module defines that
//! file format and provides platform-independent read/write codecs plus
//! Windows-specific helpers.
//!
//! ## Binary layout
//!
//! ```text
//! ┌──────────┬──────────┬──────────┬───────────────┐
//! │  magic   │ version  │  action  │  path count   │
//! │  4 bytes │  1 byte  │  1 byte  │  4 bytes LE   │
//! ├──────────┴──────────┴──────────┴───────────────┤
//! │  per-path entries (count times):               │
//! │  ┌────────────────┬──────────────────────┐     │
//! │  │ length (u32 LE)│ UTF-16LE code units  │     │
//! │  │ 4 bytes        │ 2 × length bytes     │     │
//! │  └────────────────┴──────────────────────┘     │
//! └────────────────────────────────────────────────┘
//! ```
//!
//! - **magic**: `b"GZSA"` (GeeZip Shell Action).
//! - **version**: `0x01`.  Unknown versions are rejected.
//! - **action**: `0x01` = Compress, `0x02` = CompressZip.
//! - **path count**: `u32` LE, must be ≥ 1 and ≤ MAX_PATH_COUNT.
//! - **length**: UTF-16 code-unit count (NOT byte count), `u32` LE.
//! - **data**: UTF-16LE bytes, no BOM, no null terminator stored.
//!
//! All multi-byte integers are little-endian.
//!
//! ## Hard limits (checked before allocation)
//!
//! | Limit              | Value    | Rationale                             |
//! |--------------------|----------|---------------------------------------|
//! | Max file size      | 1 MiB    | Prevents DoS via huge selection.      |
//! | Max path count     | 10 000   | Well beyond practical Explorer limits.|
//! | Max path code units| 32 767   | Aligned with `PATH_MAX`-like bounds.  |
//!
//! ## Security
//!
//! - The file is read **only** from the expected directory
//!   (`%LOCALAPPDATA%\GeeZipX\ShellActions`).
//! - The filename must have the expected `.gzsa` extension.
//! - All limits are checked **before** allocation — truncated, malformed,
//!   or maliciously crafted files are rejected with a descriptive error.
//! - After a successful read the file is best-effort deleted.
//! - This module does **not** implement the `--shell-action-file` CLI flag
//!   validation — that belongs to the argument parser in `lib.rs`.

use std::fmt;
use std::io;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Magic bytes that identify a GeeZipX shell action file.
pub const MAGIC: &[u8; 4] = b"GZSA";

/// Current protocol version.  Increment when the binary layout changes
/// incompatibly.
pub const VERSION: u8 = 0x01;

/// Maximum total file size in bytes (1 MiB).
pub const MAX_FILE_SIZE: u64 = 1_048_576;

/// Maximum number of paths in one action file.
pub const MAX_PATH_COUNT: u32 = 10_000;

/// Maximum UTF-16 code units per single path.
pub const MAX_PATH_UNITS: u32 = 32_767;

/// Expected file extension for action files.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub const FILE_EXTENSION: &str = "gzsa";

/// Subdirectory under `%LOCALAPPDATA%\GeeZipX` where action files are written.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub const SHELL_ACTIONS_DIR: &str = "ShellActions";

// ---------------------------------------------------------------------------
// Action enum
// ---------------------------------------------------------------------------

/// Shell action stored in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellActionFileAction {
    /// Jump to compress page with paths pre-filled.
    Compress = 0x01,
    /// Headless quick ZIP compress.
    CompressZip = 0x02,
}

impl ShellActionFileAction {
    /// Convert to the string action name expected by the frontend.
    pub fn as_action_str(self) -> &'static str {
        match self {
            ShellActionFileAction::Compress => "compress",
            ShellActionFileAction::CompressZip => "compress-zip",
        }
    }

    /// Decode from a byte.
    fn from_byte(b: u8) -> Result<Self, Error> {
        match b {
            0x01 => Ok(Self::Compress),
            0x02 => Ok(Self::CompressZip),
            other => Err(Error::UnknownAction(other)),
        }
    }
}

impl fmt::Display for ShellActionFileAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_action_str())
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when reading or writing an action file.
#[derive(Debug)]
pub enum Error {
    /// I/O error from the underlying filesystem.
    Io(io::Error),
    /// File does not start with the expected magic bytes.
    BadMagic([u8; 4]),
    /// Unknown protocol version (only `0x01` is recognised).
    UnknownVersion(u8),
    /// Unknown action byte.
    UnknownAction(u8),
    /// Path count is zero (empty selection).
    EmptySelection,
    /// Path count exceeds `MAX_PATH_COUNT`.
    TooManyPaths(u32),
    /// A single path's UTF-16 code-unit count exceeds `MAX_PATH_UNITS`.
    PathTooLong(u32),
    /// File is truncated — not enough bytes to satisfy the declared layout.
    Truncated,
    /// Extra trailing bytes follow the last path entry.
    TrailingData(usize),
    /// File size exceeds `MAX_FILE_SIZE`.
    FileTooLarge(u64),
    /// A path could not be decoded from the stored UTF-16LE bytes.
    InvalidUtf16,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::BadMagic(b) => write!(f, "bad magic bytes: {b:02x?}"),
            Error::UnknownVersion(v) => write!(f, "unknown version: {v:#04x}"),
            Error::UnknownAction(a) => write!(f, "unknown action: {a:#04x}"),
            Error::EmptySelection => write!(f, "empty selection (path count is zero)"),
            Error::TooManyPaths(n) => write!(f, "too many paths: {n} (max {MAX_PATH_COUNT})"),
            Error::PathTooLong(n) => {
                write!(f, "path too long: {n} code units (max {MAX_PATH_UNITS})")
            }
            Error::Truncated => write!(f, "file truncated — unexpected end of data"),
            Error::TrailingData(n) => write!(f, "trailing data: {n} bytes after last path"),
            Error::FileTooLarge(n) => write!(f, "file too large: {n} bytes (max {MAX_FILE_SIZE})"),
            Error::InvalidUtf16 => write!(f, "invalid UTF-16 sequence in path data"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Platform-independent codec: Vec<Vec<u16>>
// ---------------------------------------------------------------------------

/// Encode an action + path list into the binary format.
///
/// `paths` is a list of UTF-16 code-unit sequences (one per path).  On
/// Windows these are produced by [`std::os::windows::ffi::OsStrExt::encode_wide`];
/// on non-Windows test environments you can construct them manually from
/// `&[u16]` slices.
///
/// Returns the complete binary file contents as a `Vec<u8>`.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub fn encode(action: ShellActionFileAction, paths: &[Vec<u16>]) -> Result<Vec<u8>, Error> {
    let count: u32 = paths
        .len()
        .try_into()
        .map_err(|_| Error::TooManyPaths(paths.len() as u32))?;

    if count == 0 {
        return Err(Error::EmptySelection);
    }
    if count > MAX_PATH_COUNT {
        return Err(Error::TooManyPaths(count));
    }

    for (i, p) in paths.iter().enumerate() {
        let units: u32 = p
            .len()
            .try_into()
            .map_err(|_| Error::PathTooLong(p.len() as u32))?;
        if units > MAX_PATH_UNITS {
            return Err(Error::PathTooLong(units));
        }
        if units == 0 {
            return Err(Error::InvalidUtf16); // empty path = invalid
        }
        // Defensive: guard against silent truncation in length calculation.
        // Each code unit is 2 bytes; a huge path can overflow the per-path
        // size accumulator but that is capped by the global MAX_FILE_SIZE
        // check applied later (in the writer).
        let _ = (units, i); // suppress unused warning
    }

    // Pre-allocate: header (10) + per-path overhead (4 each) + data.
    let header_size: usize = 4 + 1 + 1 + 4; // magic + version + action + count
    let mut cap: usize = header_size;
    for p in paths {
        cap = cap.saturating_add(4 + p.len().saturating_mul(2));
    }
    if cap > MAX_FILE_SIZE as usize {
        return Err(Error::FileTooLarge(cap as u64));
    }

    let mut buf = Vec::with_capacity(cap);
    buf.extend_from_slice(MAGIC);
    buf.push(VERSION);
    buf.push(action as u8);
    buf.extend_from_slice(&count.to_le_bytes());

    for p in paths {
        let units = p.len() as u32;
        buf.extend_from_slice(&units.to_le_bytes());
        for &cu in p {
            buf.extend_from_slice(&cu.to_le_bytes());
        }
    }

    Ok(buf)
}

/// Decode a binary action file into an action + path list.
///
/// The returned paths are `Vec<Vec<u16>>` — platform-independent UTF-16
/// code-unit sequences.  On Windows convert with
/// [`std::os::windows::ffi::OsStringExt::from_wide`]; on other platforms
/// use [`String::from_utf16_lossy`] for display/debug.
///
/// `data` must be the complete file contents.
pub fn decode(data: &[u8]) -> Result<(ShellActionFileAction, Vec<Vec<u16>>), Error> {
    // --- size guard --------------------------------------------------------
    if data.len() > MAX_FILE_SIZE as usize {
        return Err(Error::FileTooLarge(data.len() as u64));
    }

    // --- header ------------------------------------------------------------
    if data.len() < 10 {
        return Err(Error::Truncated);
    }

    // Magic
    let magic = [data[0], data[1], data[2], data[3]];
    if &magic != MAGIC {
        return Err(Error::BadMagic(magic));
    }

    // Version
    let version = data[4];
    if version != VERSION {
        return Err(Error::UnknownVersion(version));
    }

    // Action
    let action = ShellActionFileAction::from_byte(data[5])?;

    // Path count
    let count = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
    if count == 0 {
        return Err(Error::EmptySelection);
    }
    if count > MAX_PATH_COUNT {
        return Err(Error::TooManyPaths(count));
    }

    // --- paths -------------------------------------------------------------
    let mut paths: Vec<Vec<u16>> = Vec::with_capacity(count as usize);
    let mut offset: usize = 10;

    for _ in 0..count {
        // Length prefix
        if offset + 4 > data.len() {
            return Err(Error::Truncated);
        }
        let units = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        if units == 0 {
            return Err(Error::InvalidUtf16);
        }
        if units > MAX_PATH_UNITS {
            return Err(Error::PathTooLong(units));
        }

        // Data
        let byte_len = (units as usize)
            .checked_mul(2)
            .ok_or(Error::PathTooLong(units))?;
        if offset + byte_len > data.len() {
            return Err(Error::Truncated);
        }

        let raw = &data[offset..offset + byte_len];
        // Decode UTF-16LE code units safely — works on any endianness.
        let mut path: Vec<u16> = Vec::with_capacity(units as usize);
        let mut pos = 0;
        while pos + 2 <= raw.len() {
            path.push(u16::from_le_bytes([raw[pos], raw[pos + 1]]));
            pos += 2;
        }
        paths.push(path);
        offset += byte_len;
    }

    // --- trailing data check -----------------------------------------------
    if offset < data.len() {
        return Err(Error::TrailingData(data.len() - offset));
    }

    Ok((action, paths))
}

// ===========================================================================
// Windows-specific helpers
// ===========================================================================

/// Write an action file for the given action and paths to the ShellActions
/// directory under `%LOCALAPPDATA%\GeeZipX`.
///
/// Creates the directory if it doesn't exist.  The filename is
/// `{pid}_{timestamp}_{counter}.gzsa` to avoid collisions.
///
/// Returns the full path to the written file on success.
#[cfg(target_os = "windows")]
pub fn write_action_file(
    action: ShellActionFileAction,
    paths: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, Error> {
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;

    // Convert PathBufs to Vec<Vec<u16>>
    let wide_paths: Vec<Vec<u16>> = paths
        .iter()
        .map(|pb| pb.as_os_str().encode_wide().collect::<Vec<u16>>())
        .collect();

    let data = encode(action, &wide_paths)?;

    // Determine directory
    let dir = shell_actions_dir()?;
    std::fs::create_dir_all(&dir)?;

    // Generate collision-safe filename: pid_timestamp_counter.gzsa
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Atomic counter — used only to generate a candidate filename component.
    // `Ordering::Relaxed` is sufficient because the counter does not guard
    // any shared state: the actual collision safety comes from `create_new(true)`
    // in `OpenOptions`, which atomically fails if the file already exists.
    // The counter just makes collisions statistically unlikely in the first place.
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let ctr = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let filename = format!("{pid}_{ts}_{ctr}.{FILE_EXTENSION}");
    let filepath = dir.join(&filename);

    // Write atomically: create_new fails if file exists; write all bytes;
    // then the file is complete (no partial-read window).
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&filepath)?;
    f.write_all(&data)?;
    f.flush()?;

    Ok(filepath)
}

/// Return the `%LOCALAPPDATA%\GeeZipX\ShellActions` directory path.
#[cfg(target_os = "windows")]
fn shell_actions_dir() -> Result<std::path::PathBuf, Error> {
    let local_appdata = std::env::var("LOCALAPPDATA").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "LOCALAPPDATA environment variable not set",
        )
    })?;
    Ok(std::path::PathBuf::from(local_appdata)
        .join("GeeZipX")
        .join(SHELL_ACTIONS_DIR))
}

/// Read and decode an action file, returning the action + decoded paths as
/// `PathBuf`s.  The file is best-effort deleted on success.
///
/// On error the file is **not** deleted — a malformed file may be evidence
/// of a bug or disk corruption and should be preserved for diagnostics.
///
/// `expected_dir` is the directory the file must reside in (after symlink
/// resolution).
///
/// On Windows, path conversion uses [`std::os::windows::ffi::OsStringExt::from_wide`]
/// for correctness.  On non-Windows (test-only), paths are converted via
/// [`String::from_utf16_lossy`].
#[cfg(any(target_os = "windows", test))]
pub(crate) fn read_action_file_in_dir(
    path: &std::path::Path,
    expected_dir: &std::path::Path,
) -> Result<(ShellActionFileAction, Vec<std::path::PathBuf>), Error> {
    // --- path validation ---------------------------------------------------
    validate_action_file_path_in_dir(path, expected_dir)?;

    // --- size check before reading -----------------------------------------
    let meta = std::fs::metadata(path)?;
    let file_size = meta.len();
    if file_size > MAX_FILE_SIZE {
        return Err(Error::FileTooLarge(file_size));
    }

    // --- read --------------------------------------------------------------
    let data = std::fs::read(path)?;
    let (action, wide_paths) = decode(&data)?;

    // --- convert to PathBufs -----------------------------------------------
    #[cfg(target_os = "windows")]
    let paths: Vec<std::path::PathBuf> = {
        use std::os::windows::ffi::OsStringExt;
        wide_paths
            .into_iter()
            .map(|w| std::path::PathBuf::from(std::ffi::OsString::from_wide(&w)))
            .collect()
    };
    #[cfg(not(target_os = "windows"))]
    let paths: Vec<std::path::PathBuf> = wide_paths
        .into_iter()
        .map(|w| std::path::PathBuf::from(String::from_utf16_lossy(&w)))
        .collect();

    // --- best-effort delete ------------------------------------------------
    // Only deleted on success — malformed files are left for diagnostics.
    let _ = std::fs::remove_file(path);

    Ok((action, paths))
}

/// Read and decode an action file, returning the action + decoded paths as
/// `PathBuf`s.  The file is best-effort deleted on success.
///
/// The `path` must point to a file within the expected ShellActions directory
/// with a `.gzsa` extension.  Any deviation returns an error.
#[cfg(target_os = "windows")]
pub fn read_action_file(
    path: &std::path::Path,
) -> Result<(ShellActionFileAction, Vec<std::path::PathBuf>), Error> {
    let expected_dir = shell_actions_dir()?;
    read_action_file_in_dir(path, &expected_dir)
}

/// Validate that `path` is inside `expected_dir` and has the expected
/// extension.  Also checks that the file itself (if it already exists)
/// does not resolve outside the directory via symlinks or junctions.
///
/// This is the injectable core — `validate_action_file_path` delegates
/// here with the real `%LOCALAPPDATA%\GeeZipX\ShellActions` directory.
/// Tests can call this directly with a temporary directory.
///
/// Available on all platforms for testing; the Windows-specific
/// `OsStringExt` conversion lives in `read_action_file_in_dir`.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn validate_action_file_path_in_dir(
    path: &std::path::Path,
    expected_dir: &std::path::Path,
) -> Result<(), Error> {
    // Extension check
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case(FILE_EXTENSION) => {}
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "action file must have .{FILE_EXTENSION} extension: {}",
                    path.display()
                ),
            )
            .into());
        }
    }

    let canonical_expected = std::fs::canonicalize(expected_dir).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "expected directory does not exist or is inaccessible: {}: {e}",
                expected_dir.display()
            ),
        )
    })?;

    // If the file already exists, canonicalize the file itself to detect
    // symlinks / junctions that point outside the expected directory.
    // This prevents reading (and subsequently deleting) an action file
    // that resolves outside the ShellActions tree.
    if let Ok(canonical_file) = std::fs::canonicalize(path) {
        if !canonical_file.starts_with(&canonical_expected) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "action file resolves outside target directory: {} -> {}",
                    path.display(),
                    canonical_file.display(),
                ),
            )
            .into());
        }
    }

    // Canonicalize the parent of the file to check it's in the right dir.
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("action file path has no parent: {}", path.display()),
        )
    })?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "action file directory does not exist or is inaccessible: {}",
                parent.display()
            ),
        )
    })?;

    if canonical_parent != canonical_expected {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "action file must be in the expected directory: {} (expected {})",
                path.display(),
                canonical_expected.display(),
            ),
        )
        .into());
    }

    Ok(())
}

/// Validate that `path` is inside the expected ShellActions directory and
/// has the expected extension.  Also checks that the file itself (if it
/// already exists) does not resolve outside the directory via symlinks or
/// junctions.
#[cfg(target_os = "windows")]
fn validate_action_file_path(path: &std::path::Path) -> Result<(), Error> {
    let expected_dir = shell_actions_dir()?;
    validate_action_file_path_in_dir(path, &expected_dir)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build a test path from a string literal as Vec<u16>
    fn path_u16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    // -- round-trip ---------------------------------------------------------

    #[test]
    fn test_round_trip_compress() {
        let paths: Vec<Vec<u16>> = vec![
            path_u16(r"C:\Users\test\file1.txt"),
            path_u16(r"C:\Users\test\文档.docx"),
        ];
        let data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        let (action, decoded) = decode(&data).unwrap();
        assert_eq!(action, ShellActionFileAction::Compress);
        assert_eq!(decoded, paths);
    }

    #[test]
    fn test_round_trip_compress_zip() {
        let paths: Vec<Vec<u16>> = vec![path_u16(r"D:\data\archive.7z")];
        let data = encode(ShellActionFileAction::CompressZip, &paths).unwrap();
        let (action, decoded) = decode(&data).unwrap();
        assert_eq!(action, ShellActionFileAction::CompressZip);
        assert_eq!(decoded, paths);
    }

    #[test]
    fn test_round_trip_empty_selection_rejected() {
        let paths: Vec<Vec<u16>> = vec![];
        let err = encode(ShellActionFileAction::Compress, &paths).unwrap_err();
        assert!(matches!(err, Error::EmptySelection));
    }

    // -- bad magic ----------------------------------------------------------

    #[test]
    fn test_bad_magic() {
        let mut data = vec![0u8; 10];
        data[0] = b'X';
        data[1] = b'X';
        data[2] = b'X';
        data[3] = b'X';
        data[4] = VERSION;
        data[5] = 0x01;
        data[6..10].copy_from_slice(&1u32.to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::BadMagic(_)));
    }

    // -- bad version --------------------------------------------------------

    #[test]
    fn test_unknown_version() {
        let mut data = vec![0u8; 10];
        data[0..4].copy_from_slice(MAGIC);
        data[4] = 0xFF; // bad version
        data[5] = 0x01;
        data[6..10].copy_from_slice(&1u32.to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::UnknownVersion(0xFF)));
    }

    // -- bad action ---------------------------------------------------------

    #[test]
    fn test_unknown_action() {
        let mut data = vec![0u8; 10];
        data[0..4].copy_from_slice(MAGIC);
        data[4] = VERSION;
        data[5] = 0xFF; // bad action
        data[6..10].copy_from_slice(&1u32.to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::UnknownAction(0xFF)));
    }

    // -- truncated ----------------------------------------------------------

    #[test]
    fn test_truncated_header() {
        let data = vec![0u8; 5]; // less than 10 bytes
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::Truncated));
    }

    #[test]
    fn test_truncated_path_length_field() {
        // Header says 1 path, but no path data follows.
        let mut data = vec![0u8; 10];
        data[0..4].copy_from_slice(MAGIC);
        data[4] = VERSION;
        data[5] = 0x01;
        data[6..10].copy_from_slice(&1u32.to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::Truncated));
    }

    #[test]
    fn test_truncated_path_data() {
        // Header says 1 path with 10 code units, but only 5 bytes follow.
        let mut data = vec![0u8; 10 + 4 + 5]; // header + length + only 5 data bytes
        data[0..4].copy_from_slice(MAGIC);
        data[4] = VERSION;
        data[5] = 0x01;
        data[6..10].copy_from_slice(&1u32.to_le_bytes());
        data[10..14].copy_from_slice(&10u32.to_le_bytes()); // claims 10 units (20 bytes)
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::Truncated));
    }

    // -- trailing data ------------------------------------------------------

    #[test]
    fn test_trailing_data() {
        let paths: Vec<Vec<u16>> = vec![path_u16(r"C:\test.txt")];
        let mut data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        // Append extra bytes
        data.push(0xAA);
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::TrailingData(1)));
    }

    // -- too many paths -----------------------------------------------------

    #[test]
    fn test_too_many_paths_in_encode() {
        let paths: Vec<Vec<u16>> = vec![path_u16("/x"); (MAX_PATH_COUNT as usize) + 1];
        let err = encode(ShellActionFileAction::Compress, &paths).unwrap_err();
        assert!(matches!(err, Error::TooManyPaths(_)));
    }

    #[test]
    fn test_too_many_paths_in_decode() {
        let mut data = vec![0u8; 10];
        data[0..4].copy_from_slice(MAGIC);
        data[4] = VERSION;
        data[5] = 0x01;
        data[6..10].copy_from_slice(&(MAX_PATH_COUNT + 1).to_le_bytes());
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::TooManyPaths(n) if n == MAX_PATH_COUNT + 1));
    }

    // -- path too long ------------------------------------------------------

    #[test]
    fn test_path_too_long_in_encode() {
        let long: Vec<u16> = vec![b'A' as u16; (MAX_PATH_UNITS as usize) + 1];
        let paths = vec![long];
        let err = encode(ShellActionFileAction::Compress, &paths).unwrap_err();
        assert!(matches!(err, Error::PathTooLong(_)));
    }

    #[test]
    fn test_path_too_long_in_decode() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(VERSION);
        data.push(0x01);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&(MAX_PATH_UNITS + 1).to_le_bytes());
        // Pad with some bytes so the truncation check doesn't fire first.
        data.resize(data.len() + ((MAX_PATH_UNITS as usize + 1) * 2), 0);
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::PathTooLong(_)));
    }

    // -- file too large -----------------------------------------------------

    #[test]
    fn test_file_too_large_in_decode() {
        let data = vec![0u8; (MAX_FILE_SIZE as usize) + 1];
        let err = decode(&data).unwrap_err();
        assert!(matches!(err, Error::FileTooLarge(_)));
    }

    #[test]
    fn test_file_too_large_in_encode() {
        // Many paths that individually pass MAX_PATH_UNITS but whose
        // combined encoded size exceeds MAX_FILE_SIZE (1 MiB).
        // header (10) + N × (4 + 2×units) must exceed 1 048 576.
        // Use 6 000 paths of 100 code units each:
        //   10 + 6000 × (4 + 200) = 1 224 010 > 1 048 576.
        let path: Vec<u16> = vec![b'X' as u16; 100];
        let paths: Vec<Vec<u16>> = (0..6000).map(|_| path.clone()).collect();
        let err = encode(ShellActionFileAction::Compress, &paths).unwrap_err();
        assert!(
            matches!(err, Error::FileTooLarge(_)),
            "expected FileTooLarge, got {err:?}"
        );
    }

    // -- UTF-16 surrogate code units (valid UTF-16, should round-trip) -----

    #[test]
    fn test_surrogate_code_units() {
        // U+1F600 (grinning face) = surrogate pair [0xD83D, 0xDE00]
        let path: Vec<u16> = vec![
            0xD83D,
            0xDE00,
            b'.' as u16,
            b't' as u16,
            b'x' as u16,
            b't' as u16,
        ];
        let paths = vec![path.clone()];
        let data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        let (_, decoded) = decode(&data).unwrap();
        assert_eq!(decoded[0], path);
    }

    // -- long path (near MAX_PATH_UNITS) ------------------------------------

    #[test]
    fn test_long_path_near_limit() {
        let long: Vec<u16> = vec![b'X' as u16; MAX_PATH_UNITS as usize];
        let paths = vec![long.clone()];
        let data = encode(ShellActionFileAction::CompressZip, &paths).unwrap();
        let (_, decoded) = decode(&data).unwrap();
        assert_eq!(decoded[0], long);
        assert_eq!(decoded[0].len(), MAX_PATH_UNITS as usize);
    }

    // -- max path count -----------------------------------------------------

    #[test]
    fn test_max_path_count_ok() {
        // Each path is 1 code unit (2 bytes).  10_000 paths × (4+2) = 60_000 + header.
        let paths: Vec<Vec<u16>> = (0..MAX_PATH_COUNT)
            .map(|i| vec![(b'a' + (i % 26) as u8) as u16])
            .collect();
        let data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        let (_, decoded) = decode(&data).unwrap();
        assert_eq!(decoded.len(), MAX_PATH_COUNT as usize);
    }

    // -- zero-length path inside list ---------------------------------------

    #[test]
    fn test_zero_length_path_rejected() {
        let paths: Vec<Vec<u16>> = vec![path_u16(r"C:\ok.txt"), vec![]];
        let err = encode(ShellActionFileAction::Compress, &paths).unwrap_err();
        assert!(matches!(err, Error::InvalidUtf16));
    }

    // -- action string conversion -------------------------------------------

    #[test]
    fn test_action_as_str() {
        assert_eq!(ShellActionFileAction::Compress.as_action_str(), "compress");
        assert_eq!(
            ShellActionFileAction::CompressZip.as_action_str(),
            "compress-zip"
        );
    }

    // -- magic constant -----------------------------------------------------

    #[test]
    fn test_magic_is_four_bytes() {
        assert_eq!(MAGIC.len(), 4);
        assert_eq!(MAGIC, b"GZSA");
    }

    // -- version constant ---------------------------------------------------

    #[test]
    fn test_version_is_1() {
        assert_eq!(VERSION, 0x01);
    }

    // ======================================================================
    // Real file I/O tests (Windows-only — uses write_action_file & OsStringExt)
    // ======================================================================

    #[test]
    fn test_write_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        // We can't easily inject a custom directory into write_action_file
        // (it reads LOCALAPPDATA), so we use encode + manual file write
        // and then test read_action_file_in_dir.
        let paths: Vec<Vec<u16>> = vec![
            "C:\\Users\\test\\file1.txt".encode_utf16().collect(),
            "C:\\Users\\test\\文档.docx".encode_utf16().collect(),
        ];
        let data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        let file_path = actions_dir.join("roundtrip_test.gzsa");
        std::fs::write(&file_path, &data).unwrap();

        let (action, read_paths) = read_action_file_in_dir(&file_path, &actions_dir).unwrap();
        assert_eq!(action, ShellActionFileAction::Compress);
        assert_eq!(read_paths.len(), 2);
        assert!(read_paths[0].to_string_lossy().contains("file1.txt"));
        assert!(read_paths[1].to_string_lossy().contains("文档.docx"));

        // File should be deleted after successful read.
        assert!(!file_path.exists());
    }

    #[test]
    fn test_write_read_compress_zip_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        let paths: Vec<Vec<u16>> = vec!["D:\\data\\archive.7z".encode_utf16().collect()];
        let data = encode(ShellActionFileAction::CompressZip, &paths).unwrap();
        let file_path = actions_dir.join("zip_test.gzsa");
        std::fs::write(&file_path, &data).unwrap();

        let (action, read_paths) = read_action_file_in_dir(&file_path, &actions_dir).unwrap();
        assert_eq!(action, ShellActionFileAction::CompressZip);
        assert_eq!(read_paths.len(), 1);
        assert!(!file_path.exists());
    }

    /// Corrupted action files should return an error and NOT be deleted
    /// (preserved for diagnostics).
    #[test]
    fn test_corrupted_file_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        let file_path = actions_dir.join("corrupt.gzsa");
        std::fs::write(&file_path, b"this is not a valid action file").unwrap();

        let result = read_action_file_in_dir(&file_path, &actions_dir);
        assert!(result.is_err());

        // Corrupted file must NOT be deleted — preserved for diagnostics.
        assert!(file_path.exists());
    }

    /// An action file with correct magic but truncated path data should
    /// fail decode and NOT be deleted.
    #[test]
    fn test_truncated_file_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        // Header says 5 paths with 100 code units each, but no data follows.
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(VERSION);
        data.push(0x01);
        data.extend_from_slice(&5u32.to_le_bytes());
        data.extend_from_slice(&100u32.to_le_bytes()); // first path claims 100 units; no actual path data — truncated.

        let file_path = actions_dir.join("truncated.gzsa");
        std::fs::write(&file_path, &data).unwrap();

        let result = read_action_file_in_dir(&file_path, &actions_dir);
        assert!(result.is_err());
        assert!(file_path.exists());
    }

    /// Files outside the expected directory must be rejected and NOT deleted.
    #[test]
    fn test_path_outside_dir_rejected_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        let other_dir = dir.path().join("OtherDir");
        std::fs::create_dir_all(&actions_dir).unwrap();
        std::fs::create_dir_all(&other_dir).unwrap();

        // Write a valid action file outside the expected directory.
        let paths: Vec<Vec<u16>> = vec!["C:\\test.txt".encode_utf16().collect()];
        let data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        let file_path = other_dir.join("outside.gzsa");
        std::fs::write(&file_path, &data).unwrap();

        let result = read_action_file_in_dir(&file_path, &actions_dir);
        assert!(result.is_err());

        // File outside expected dir must NOT be deleted.
        assert!(file_path.exists());
    }

    /// Wrong file extension must be rejected.
    #[test]
    fn test_wrong_extension_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        let file_path = actions_dir.join("evil.txt");
        std::fs::write(&file_path, b"not an action file").unwrap();

        let result = read_action_file_in_dir(&file_path, &actions_dir);
        assert!(result.is_err());
        assert!(file_path.exists());
    }

    /// Files exceeding MAX_FILE_SIZE must be rejected before reading.
    #[test]
    fn test_file_too_large_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        let file_path = actions_dir.join("huge.gzsa");
        // Create a file just over MAX_FILE_SIZE by using seek.
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&file_path).unwrap();
            f.write_all(MAGIC).unwrap();
            f.write_all(&[VERSION, 0x01, 0, 0, 0, 0]).unwrap();
            // Seek to MAX_FILE_SIZE + 1 and write a byte to create a sparse file.
            f.set_len(MAX_FILE_SIZE + 1).unwrap();
        }

        let result = read_action_file_in_dir(&file_path, &actions_dir);
        assert!(matches!(result, Err(Error::FileTooLarge(_))));
        assert!(file_path.exists());
    }

    /// File with 0-byte size must fail on decode (truncated).
    #[test]
    fn test_empty_file_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        let file_path = actions_dir.join("empty.gzsa");
        std::fs::write(&file_path, b"").unwrap();

        let result = read_action_file_in_dir(&file_path, &actions_dir);
        assert!(result.is_err());
        assert!(file_path.exists());
    }

    // ======================================================================
    // Path validator tests (platform-independent via temp dirs)
    // ======================================================================

    /// Symlink escape: a symlink inside the expected dir that points to a
    /// file outside must be rejected by canonicalize-based checks.
    ///
    /// Windows note: this test uses Unix `symlink`.  Windows junctions have
    /// similar semantics (canonicalize follows them), but the Windows CRT
    /// `symlink` function requires elevated privileges or Developer Mode and
    /// is not tested here.  Residual risk on Windows is noted.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_symlink_escape_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        let outside_dir = dir.path().join("Outside");
        std::fs::create_dir_all(&actions_dir).unwrap();
        std::fs::create_dir_all(&outside_dir).unwrap();

        // Create a valid file outside the expected dir.
        let outside_file = outside_dir.join("lured.gzsa");
        std::fs::write(
            &outside_file,
            b"valid magic would go here but rejected early",
        )
        .unwrap();

        // Symlink inside the expected dir → outside file.
        let symlink_path = actions_dir.join("escape.gzsa");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        let result = validate_action_file_path_in_dir(&symlink_path, &actions_dir);
        assert!(result.is_err(), "symlink escape must be rejected");
    }

    /// Valid file inside the expected dir passes validation.
    #[test]
    fn test_valid_file_in_dir_passes_validation() {
        let dir = tempfile::tempdir().unwrap();
        let actions_dir = dir.path().join("ShellActions");
        std::fs::create_dir_all(&actions_dir).unwrap();

        let file_path = actions_dir.join("valid.gzsa");
        std::fs::write(&file_path, b"dummy content").unwrap();

        let result = validate_action_file_path_in_dir(&file_path, &actions_dir);
        assert!(result.is_ok());
    }

    /// Extension check works on all platforms.
    #[test]
    fn test_extension_check() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();

        // Wrong extension
        let bad = dir.path().join("test.txt");
        std::fs::write(&bad, b"x").unwrap();
        let result = validate_action_file_path_in_dir(&bad, dir.path());
        assert!(result.is_err());

        // Correct extension
        let good = dir.path().join("test.gzsa");
        std::fs::write(&good, b"x").unwrap();
        let result = validate_action_file_path_in_dir(&good, dir.path());
        assert!(result.is_ok());

        // Uppercase extension
        let upper = dir.path().join("TEST.GZSA");
        std::fs::write(&upper, b"x").unwrap();
        let result = validate_action_file_path_in_dir(&upper, dir.path());
        assert!(result.is_ok());
    }

    /// Path without parent (e.g. bare filename) is rejected.
    #[test]
    fn test_no_parent_rejected() {
        let path = std::path::Path::new("bare.gzsa");
        let dir = tempfile::tempdir().unwrap();
        let result = validate_action_file_path_in_dir(path, dir.path());
        assert!(result.is_err());
    }

    /// Non-existent parent directory is rejected.
    #[test]
    fn test_nonexistent_parent_rejected() {
        let path = std::path::Path::new("/nonexistent/dir/subdir/file.gzsa");
        let dir = tempfile::tempdir().unwrap();
        let result = validate_action_file_path_in_dir(path, dir.path());
        assert!(result.is_err());
    }
}
