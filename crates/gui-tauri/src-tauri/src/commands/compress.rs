//! `compress_archive` command — create an archive from local files.
//!
//! Supported archive container formats: zip, tar, tar.gz, tar.zst, tar.xz.
//! Single-stream formats are intentionally rejected for the current GUI MVP.
//!
//! All heavy work runs on `tokio::task::spawn_blocking` so the Tauri event loop
//! is never blocked. Progress is emitted as `task:progress` events.

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tokio::task::spawn_blocking;

use geezipx_core::archive::tar::TarWriter;
use geezipx_core::archive::targz::TarGzWriter;
use geezipx_core::archive::tarxz::TarXzWriter;
use geezipx_core::archive::tarzst::TarZstWriter;
use geezipx_core::archive::zip::ZipWriter;
use geezipx_core::archive::ArchiveWriter;
use geezipx_core::config::CompressOptions;
use geezipx_core::detect::ArchiveFormat;
use geezipx_core::ProgressReader;

use crate::commands::progress::{is_cancelled_error, TaskKind, TaskProgressEmitter, TaskStage};
use crate::state::AppState;

const CANCELLED_MESSAGE: &str = "Operation cancelled by user";

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
    /// Input size in bytes for progress accounting (0 for directories).
    size: u64,
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Create an archive from a list of source paths.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn compress_archive(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    source_paths: Vec<String>,
    output_path: String,
    format: String,
    level: Option<u32>,
    jobs: Option<u32>,
    password: Option<String>,
    task_id: Option<String>,
) -> Result<CompressArchiveResult, String> {
    if source_paths.is_empty() {
        return Err("At least one source path is required".to_string());
    }

    let af = parse_gui_compress_format(&format)?;

    if password.is_some() && af != ArchiveFormat::Zip {
        return Err(format!(
            "Password is only supported for ZIP format; '{af}' does not support encryption when writing"
        ));
    }

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

    let cancel_token = {
        let mut tokens = state
            .cancel_tokens
            .lock()
            .map_err(|e| format!("Internal error: {e}"))?;
        let token = Arc::new(AtomicBool::new(false));
        tokens.insert(tid.clone(), token.clone());
        token
    };

    let emitter = TaskProgressEmitter::new(app, tid.clone(), TaskKind::Compress);
    emitter.emit_started("Scanning input files...");

    let result = {
        let emitter = emitter.clone();
        let cancel_token = cancel_token.clone();
        let out = output.clone();
        spawn_blocking(move || {
            let task_result = (|| -> Result<CompressArchiveResult, String> {
                let out_dir = match out.parent() {
                    Some(path) if !path.as_os_str().is_empty() => path,
                    _ => Path::new("."),
                };
                if let Ok(canonical_out) = fs::canonicalize(out_dir) {
                    for src in &sources {
                        if let Ok(real_src) = fs::canonicalize(src) {
                            if real_src.is_dir() && canonical_out.starts_with(&real_src) {
                                return Err(
                                    "Output file cannot be located inside a source directory"
                                        .to_string(),
                                );
                            }
                        }
                    }
                }

                let mut skipped = 0u64;
                let entries = collect_gui_inputs(&sources, &cancel_token, &mut skipped)?;
                let total_input_bytes = entries
                    .iter()
                    .filter(|entry| !entry.is_dir)
                    .map(|entry| entry.size)
                    .sum::<u64>();
                let total_entries = entries.len() as u64;

                emitter.set_totals(Some(total_input_bytes), Some(total_entries));
                emitter.emit_progress(TaskStage::Compressing, None, 0, None, 0, true);

                let output_file = fs::File::create(&out)
                    .map_err(|e| format!("Cannot create output '{}': {}", out.display(), e))?;

                let options = CompressOptions {
                    level,
                    jobs,
                    password: pwd,
                };
                let mut writer: Box<dyn ArchiveWriter> =
                    create_gui_writer(output_file, af, options)?;

                let mut added_files = 0u64;
                let mut added_dirs = 0u64;
                let mut progress_bytes = 0u64;
                let mut completed_entries = 0u64;

                for entry in &entries {
                    if cancel_token.load(Ordering::SeqCst) {
                        return Err(CANCELLED_MESSAGE.to_string());
                    }

                    let entry_name = entry.archive_path.to_string_lossy().to_string();
                    if entry.is_dir {
                        writer.add_directory(&entry.archive_path).map_err(|e| {
                            format!(
                                "Failed to add directory '{}': {e}",
                                entry.archive_path.display()
                            )
                        })?;
                        added_dirs += 1;
                        completed_entries += 1;
                        emitter.emit_progress(
                            TaskStage::Compressing,
                            None,
                            progress_bytes,
                            Some(&entry_name),
                            completed_entries,
                            true,
                        );
                        continue;
                    }

                    let file = match fs::File::open(&entry.real_path) {
                        Ok(file) => file,
                        Err(_) => {
                            skipped += 1;
                            progress_bytes = progress_bytes.saturating_add(entry.size);
                            completed_entries += 1;
                            emitter.emit_progress(
                                TaskStage::Compressing,
                                None,
                                progress_bytes,
                                Some(&entry_name),
                                completed_entries,
                                true,
                            );
                            continue;
                        }
                    };

                    let callback = emitter.reader_callback(
                        cancel_token.clone(),
                        TaskStage::Compressing,
                        progress_bytes,
                        entry_name.clone(),
                        completed_entries,
                    );
                    let mut reader = ProgressReader::new(BufReader::new(file))
                        .with_total(entry.size)
                        .with_callback(Box::new(callback));

                    match writer.add_entry_from_reader(&entry.archive_path, &mut reader) {
                        Ok(()) => {
                            added_files += 1;
                            progress_bytes = progress_bytes.saturating_add(entry.size);
                            completed_entries += 1;
                            emitter.emit_progress(
                                TaskStage::Compressing,
                                None,
                                progress_bytes,
                                Some(&entry_name),
                                completed_entries,
                                true,
                            );
                        }
                        Err(err) => {
                            if cancel_token.load(Ordering::SeqCst) || is_cancelled_error(&err) {
                                return Err(CANCELLED_MESSAGE.to_string());
                            }
                            return Err(format!(
                                "Failed to add '{}': {err}",
                                entry.archive_path.display()
                            ));
                        }
                    }
                }

                emitter.emit_finalizing(progress_bytes, completed_entries);
                let bytes_written = writer
                    .finish()
                    .map_err(|e| format!("Failed to finalize archive: {e}"))?;
                emitter.emit_finished(progress_bytes, completed_entries);

                Ok(CompressArchiveResult {
                    files_added: added_files,
                    directories_added: added_dirs,
                    bytes_written,
                    output_path: out.to_string_lossy().to_string(),
                    format: af.to_string(),
                    skipped,
                })
            })();

            match task_result {
                Ok(result) => Ok(result),
                Err(message) => {
                    let _ = fs::remove_file(&out);
                    let (current, completed_entries) = emitter.latest_snapshot();
                    if cancel_token.load(Ordering::SeqCst)
                        || message.to_ascii_lowercase().contains("cancelled")
                    {
                        emitter.emit_cancelled(current, completed_entries);
                    } else {
                        emitter.emit_failed(current, completed_entries, message.clone());
                    }
                    Err(message)
                }
            }
        })
        .await
    };

    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    tokens.remove(&tid);
    drop(tokens);

    result.map_err(|e| format!("Internal error: {e}"))?
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a format string accepted by the GUI compress command.
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
fn collect_gui_inputs(
    sources: &[PathBuf],
    cancel_token: &Arc<AtomicBool>,
    skipped: &mut u64,
) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();
    for source in sources {
        if cancel_token.load(Ordering::SeqCst) {
            return Err(CANCELLED_MESSAGE.to_string());
        }

        let meta = fs::symlink_metadata(source)
            .map_err(|e| format!("Cannot inspect path '{}': {}", source.display(), e))?;
        if meta.file_type().is_symlink() {
            *skipped += 1;
            continue;
        }

        let canonical = fs::canonicalize(source)
            .map_err(|e| format!("Cannot resolve path '{}': {}", source.display(), e))?;

        if meta.is_dir() {
            let dir_name = canonical
                .file_name()
                .unwrap_or(canonical.as_os_str())
                .to_os_string();
            let prefix = PathBuf::from(&dir_name);
            entries.push(FileEntry {
                real_path: canonical.clone(),
                archive_path: prefix.clone(),
                is_dir: true,
                size: 0,
            });
            collect_dir_contents(&canonical, &prefix, &mut entries, cancel_token, skipped)?;
        } else if meta.is_file() {
            let name = canonical
                .file_name()
                .unwrap_or(canonical.as_os_str())
                .to_os_string();
            entries.push(FileEntry {
                real_path: canonical,
                archive_path: PathBuf::from(name),
                is_dir: false,
                size: meta.len(),
            });
        } else {
            *skipped += 1;
        }
    }
    Ok(entries)
}

/// Recursively walk `dir` and append entries, prepending `prefix`.
fn collect_dir_contents(
    dir: &Path,
    prefix: &Path,
    entries: &mut Vec<FileEntry>,
    cancel_token: &Arc<AtomicBool>,
    skipped: &mut u64,
) -> Result<(), String> {
    let read_dir = fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory '{}': {}", dir.display(), e))?;

    for entry in read_dir {
        if cancel_token.load(Ordering::SeqCst) {
            return Err(CANCELLED_MESSAGE.to_string());
        }

        let entry = entry.map_err(|e| format!("Error reading directory entry: {e}"))?;
        let path = entry.path();
        let relative = prefix.join(entry.file_name());

        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => {
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
                size: 0,
            });
            collect_dir_contents(&path, &relative, entries, cancel_token, skipped)?;
        } else if meta.is_file() {
            entries.push(FileEntry {
                real_path: path,
                archive_path: relative,
                is_dir: false,
                size: meta.len(),
            });
        } else {
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
