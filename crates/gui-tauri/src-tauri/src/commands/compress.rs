//! `compress_archive` command — create an archive from local files.
//!
//! Supported archive container formats: zip, tar, tar.gz, tar.zst, tar.xz.
//! Single-stream formats (gzip, zstd, xz, lzma) are **not** yet supported
//! for GUI compression — the command returns a clear error message.
//!
//! All heavy work runs on `tokio::task::spawn_blocking` so the Tauri event
//! loop is never blocked.  A cancellation token registered in `AppState`
//! lets the frontend abort an in-flight compression.

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tokio::task::spawn_blocking;

use geezipx_core::archive::tar::TarWriter;
use geezipx_core::archive::targz::TarGzWriter;
use geezipx_core::archive::tarxz::TarXzWriter;
use geezipx_core::archive::tarzst::TarZstWriter;
use geezipx_core::archive::zip::ZipWriter;
use geezipx_core::archive::ArchiveWriter;
use geezipx_core::config::CompressOptions;
use geezipx_core::detect::ArchiveFormat;

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Serializable result of a compression operation.
#[derive(Debug, Serialize)]
pub struct CompressArchiveResult {
    /// Number of regular files added to the archive.
    pub files_added: u64,
    /// Number of directory entries added.
    pub directories_added: u64,
    /// Total compressed bytes written to the output file.
    pub bytes_written: u64,
    /// Absolute path to the created output file.
    pub output_path: String,
    /// The format used (e.g. "zip", "tar.gz").
    pub format: String,
    /// Number of items skipped (e.g. symlinks, special files).
    pub skipped: u64,
}

/// A collected file entry ready to be added to an archive.
struct FileEntry {
    /// Real path on the filesystem.
    real_path: PathBuf,
    /// Relative path inside the archive.
    archive_path: PathBuf,
    /// Whether this entry is a directory (not a regular file).
    is_dir: bool,
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Create an archive from a list of source paths.
///
/// ## Supported formats (archive containers)
///
/// - `zip`
/// - `tar`
/// - `tar.gz`
/// - `tar.zst`
/// - `tar.xz`
///
/// ## Unsupported in this command
///
/// Single-stream formats (`gzip`, `zstd`, `xz`, `lzma`) and read-only
/// archive formats (`7z`, `rar`) return a clear error.  Single-stream
/// compression will be added in a later update.
///
/// ## Cancellation
///
/// If `task_id` is provided the command registers a cancellation token in
/// [`AppState::cancel_tokens`].  The frontend can call `cancel_task` with
/// the same id to abort the operation.  The token is always cleaned up
/// after the command completes (success, error, or cancellation).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn compress_archive(
    state: tauri::State<'_, AppState>,
    source_paths: Vec<String>,
    output_path: String,
    format: String,
    level: Option<u32>,
    jobs: Option<u32>,
    password: Option<String>,
    task_id: Option<String>,
) -> Result<CompressArchiveResult, String> {
    // --- Validate inputs ---
    if source_paths.is_empty() {
        return Err("At least one source path is required".to_string());
    }

    let af = parse_gui_compress_format(&format)?;

    // Password: only supported for ZIP when writing.
    if password.is_some() && af != ArchiveFormat::Zip {
        return Err(format!(
            "Password is only supported for ZIP format; '{af}' does not support encryption when writing"
        ));
    }

    // Generate a task id if not provided by the frontend.
    let tid = task_id.unwrap_or_else(|| {
        format!(
            "compress-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });

    let output = PathBuf::from(&output_path);
    let sources: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let pwd = password;

    // --- Register cancellation token ---
    let cancel_token = {
        let mut tokens = state
            .cancel_tokens
            .lock()
            .map_err(|e| format!("Internal error: {}", e))?;
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        tokens.insert(tid.clone(), token.clone());
        token
    };

    let fmt = af; // move into closure
    let out = output;

    // --- Run compression on the blocking pool ---
    let result = spawn_blocking(move || {
        // H1: Reject if the output file is inside any source directory.
        let out_dir = match out.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        if let Ok(canonical_out) = fs::canonicalize(out_dir) {
            for src in &sources {
                if let Ok(real_src) = fs::canonicalize(src) {
                    if real_src.is_dir() && canonical_out.starts_with(&real_src) {
                        return Err(
                            "Output file cannot be located inside a source directory".to_string()
                        );
                    }
                }
            }
        }

        // 1. Collect input files (recursive for directories).
        let mut skipped: u64 = 0;
        let entries = collect_gui_inputs(&sources, &cancel_token, &mut skipped)?;

        // 2. Create output file.
        let output_file = fs::File::create(&out)
            .map_err(|e| format!("Cannot create output '{}': {}", out.display(), e))?;

        // 3. Create the appropriate writer.
        let options = CompressOptions {
            level,
            jobs,
            password: pwd,
        };
        let mut writer: Box<dyn ArchiveWriter> = create_gui_writer(output_file, fmt, options)?;

        // 4. Add each entry.
        let mut added_files: u64 = 0;
        let mut added_dirs: u64 = 0;

        for entry in &entries {
            // Check cancellation before each entry.
            if cancel_token.load(Ordering::SeqCst) {
                // Clean up partial output file on cancellation.
                let _ = fs::remove_file(&out);
                return Err("Operation cancelled by user".to_string());
            }

            if entry.is_dir {
                writer.add_directory(&entry.archive_path).map_err(|e| {
                    format!(
                        "Failed to add directory '{}': {e}",
                        entry.archive_path.display()
                    )
                })?;
                added_dirs += 1;
                continue;
            }

            // Read the file and add it.
            let file = match fs::File::open(&entry.real_path) {
                Ok(f) => f,
                Err(_) => {
                    // Skip files we cannot open; report via the skipped counter.
                    skipped += 1;
                    continue;
                }
            };
            let mut reader = BufReader::new(file);

            writer
                .add_entry_from_reader(&entry.archive_path, &mut reader)
                .map_err(|e| format!("Failed to add '{}': {e}", entry.archive_path.display()))?;
            added_files += 1;
        }

        // 5. Finalise the archive.
        let bytes_written = writer
            .finish()
            .map_err(|e| format!("Failed to finalize archive: {e}"))?;

        Ok(CompressArchiveResult {
            files_added: added_files,
            directories_added: added_dirs,
            bytes_written,
            output_path: out.to_string_lossy().to_string(),
            format: fmt.to_string(),
            skipped,
        })
    })
    .await;

    // --- Clean up cancellation token (always, even on panic / JoinError) ---
    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    tokens.remove(&tid);
    drop(tokens);

    let result = result.map_err(|e| format!("Internal error: {e}"))?;
    result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a format string accepted by the GUI compress command.
///
/// Only archive container formats are supported.  Single-stream formats
/// and read-only archive formats return a descriptive error.
fn parse_gui_compress_format(s: &str) -> Result<ArchiveFormat, String> {
    match s.to_ascii_lowercase().as_str() {
        "zip" => Ok(ArchiveFormat::Zip),
        "tar" => Ok(ArchiveFormat::Tar),
        "tar.gz" | "tgz" => Ok(ArchiveFormat::TarGz),
        "tar.zst" | "tzst" => Ok(ArchiveFormat::TarZst),
        "tar.xz" | "txz" => Ok(ArchiveFormat::TarXz),
        "gz" | "gzip" => Err(
            "'gzip' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "zst" | "zstd" => Err(
            "'zstd' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "xz" => Err(
            "'xz' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "lzma" => Err(
            "'lzma' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
                .to_string(),
        ),
        "7z" => Err(
            "7z writing is not supported; use list, test, or decompress for read-only 7z support"
                .to_string(),
        ),
        "rar" => Err(
            "rar writing is not supported; use list, test, or decompress for read-only rar support"
                .to_string(),
        ),
        other => Err(format!(
            "Unsupported format '{other}'; expected: zip, tar, tar.gz, tgz, tar.zst, tzst, tar.xz, txz"
        )),
    }
}

/// Collect source paths into `FileEntry` items, recursing into directories.
///
/// Symlinks are **skipped** (not followed) to avoid:
///   - Infinite loops from circular symlinks.
///   - Path traversal / Zip Slip via absolute symlinks.
///   - Cross-filesystem links.
///
/// If a symlink is encountered it is silently counted in the return value's
/// `skipped` field (propagated through the command result).
fn collect_gui_inputs(
    sources: &[PathBuf],
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
    skipped: &mut u64,
) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();
    for source in sources {
        // Check cancellation before processing each top-level source.
        if cancel_token.load(Ordering::SeqCst) {
            return Err("Operation cancelled by user".to_string());
        }

        let canonical = fs::canonicalize(source)
            .map_err(|e| format!("Cannot resolve path '{}': {}", source.display(), e))?;

        if canonical.is_dir() {
            // The directory's basename becomes the prefix for all entries inside.
            let dir_name = canonical
                .file_name()
                .unwrap_or(canonical.as_os_str())
                .to_os_string();
            let prefix = PathBuf::from(&dir_name);
            entries.push(FileEntry {
                real_path: canonical.clone(),
                archive_path: prefix.clone(),
                is_dir: true,
            });
            collect_dir_contents(&canonical, &prefix, &mut entries, cancel_token, skipped)?;
        } else if canonical.is_file() {
            // Use basename as the archive entry path.
            let name = canonical
                .file_name()
                .unwrap_or(canonical.as_os_str())
                .to_os_string();
            entries.push(FileEntry {
                real_path: canonical,
                archive_path: PathBuf::from(name),
                is_dir: false,
            });
        }
        // Symlinks and other special files are silently skipped.
    }
    Ok(entries)
}

/// Recursively walk `dir` and append entries, prepending `prefix`.
fn collect_dir_contents(
    dir: &Path,
    prefix: &Path,
    entries: &mut Vec<FileEntry>,
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
    skipped: &mut u64,
) -> Result<(), String> {
    let read_dir = fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory '{}': {}", dir.display(), e))?;

    for entry in read_dir {
        // Check cancellation between each directory entry.
        if cancel_token.load(Ordering::SeqCst) {
            return Err("Operation cancelled by user".to_string());
        }

        let entry = entry.map_err(|e| format!("Error reading directory entry: {e}"))?;
        let path = entry.path();
        let relative = prefix.join(entry.file_name());

        // Use symlink_metadata so we can detect and skip symlinks.
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => {
                // Can't read metadata; skip and count.
                *skipped += 1;
                continue;
            }
        };

        if meta.file_type().is_symlink() {
            *skipped += 1;
            continue;
        }

        if meta.is_dir() {
            entries.push(FileEntry {
                real_path: path.clone(),
                archive_path: relative.clone(),
                is_dir: true,
            });
            collect_dir_contents(&path, &relative, entries, cancel_token, skipped)?;
        } else if meta.is_file() {
            entries.push(FileEntry {
                real_path: path,
                archive_path: relative,
                is_dir: false,
            });
        } else {
            // Other file types (sockets, FIFOs, etc.) — skip and count.
            *skipped += 1;
        }
    }
    Ok(())
}

/// Create an `ArchiveWriter` for the given format and output file.
fn create_gui_writer(
    file: fs::File,
    format: ArchiveFormat,
    options: CompressOptions,
) -> Result<Box<dyn ArchiveWriter>, String> {
    // Password validation (should have been done earlier, but double-check).
    if options.password.is_some() && format != ArchiveFormat::Zip {
        return Err(format!(
            "Password is only supported for ZIP format; '{format}' does not support encryption when writing"
        ));
    }

    match format {
        ArchiveFormat::Zip => {
            let mut writer = ZipWriter::new(file);
            if let Some(ref pwd) = options.password {
                writer.set_password(pwd);
            }
            Ok(Box::new(writer))
        }
        ArchiveFormat::Tar => Ok(Box::new(TarWriter::new(file))),
        ArchiveFormat::TarGz => Ok(Box::new(TarGzWriter::new_with_options(file, options))),
        ArchiveFormat::TarZst => Ok(Box::new(TarZstWriter::new_with_options(file, options))),
        ArchiveFormat::TarXz => Ok(Box::new(TarXzWriter::new_with_options(file, options))),
        // Should not happen because parse_gui_compress_format rejects these.
        ArchiveFormat::Gzip | ArchiveFormat::Zstd | ArchiveFormat::Xz | ArchiveFormat::Lzma => {
            Err(format!(
                "'{format}' is a single-stream compression format; \
             single-stream compression is not yet supported in the GUI \
             (will be added in a later update)"
            ))
        }
        _ => Err(format!("Unsupported format for writing in GUI: {format}")),
    }
}
