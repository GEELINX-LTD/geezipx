//! `extract_archive` command — extract all entries from an archive.
//!
//! Reuses the shared helpers from the `list` module (`detect_archive_format`,
//! `open_reader`) and bridges extraction progress to the frontend via
//! `task:progress` events.
//!
//! # Single-stream formats (gzip, zstd, xz, lzma)
//!
//! These are not yet supported for extraction in the GUI. The command returns a
//! clear error directing users to a future update.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tokio::task::spawn_blocking;

use geezipx_core::archive::{check_entry_path_safety, ArchiveReader, Entry, ExtractReport};
use geezipx_core::detect::ArchiveFormat;
use geezipx_core::{GeeZipError, ProgressWriter};

use crate::commands::list::{detect_archive_format, open_reader};
use crate::commands::progress::{is_cancelled_error, TaskKind, TaskProgressEmitter, TaskStage};
use crate::state::AppState;

const CANCELLED_MESSAGE: &str = "Operation cancelled by user";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Serializable per-file error information returned to the frontend.
#[derive(Debug, Serialize)]
pub struct ExtractErrorInfo {
    /// Relative path inside the archive that caused the error.
    pub path: String,
    /// Human-readable error message.
    pub message: String,
}

/// Serializable result of an extraction operation.
#[derive(Debug, Serialize)]
pub struct ExtractArchiveResult {
    /// Number of files successfully extracted.
    pub files_extracted: u64,
    /// Total uncompressed bytes written to disk.
    pub bytes_extracted: u64,
    /// Number of files skipped (e.g. due to `overwrite=false`).
    pub files_skipped: u64,
    /// Per-file errors that did **not** abort the whole operation.
    pub errors: Vec<ExtractErrorInfo>,
}

impl From<ExtractReport> for ExtractArchiveResult {
    fn from(report: ExtractReport) -> Self {
        ExtractArchiveResult {
            files_extracted: report.files_extracted as u64,
            bytes_extracted: report.bytes_extracted,
            files_skipped: report.files_skipped as u64,
            errors: report
                .errors
                .into_iter()
                .map(|(path, err)| ExtractErrorInfo {
                    path,
                    message: err.to_string(),
                })
                .collect(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ExtractTaskError {
    Message(String),
    Cancelled,
}

impl From<String> for ExtractTaskError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Extract all entries from an archive to a directory.
#[tauri::command]
pub async fn extract_archive(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    archive_path: String,
    output_dir: String,
    overwrite: bool,
    password: Option<String>,
    task_id: Option<String>,
) -> Result<ExtractArchiveResult, String> {
    let tid = task_id.unwrap_or_else(|| {
        format!(
            "extract-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });

    let path_buf = PathBuf::from(&archive_path);
    let out_dir = PathBuf::from(&output_dir);
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

    let emitter = TaskProgressEmitter::new(app, tid.clone(), TaskKind::Extract);
    emitter.emit_started("Reading archive entries...");

    let result = {
        let emitter = emitter.clone();
        let cancel_token = cancel_token.clone();
        spawn_blocking(move || {
            let task_result = (|| -> Result<ExtractArchiveResult, ExtractTaskError> {
                let format = detect_archive_format(&path_buf)?;
                match format {
                    ArchiveFormat::Gzip
                    | ArchiveFormat::Zstd
                    | ArchiveFormat::Xz
                    | ArchiveFormat::Lzma => {
                        return Err(ExtractTaskError::Message(format!(
                            "'{format}' is a single-stream compression format; \
                             single-stream decompression is not yet supported in the GUI \
                             (will be added in a later update)"
                        )));
                    }
                    _ => {}
                }

                let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;
                let entries = reader.entries().map_err(|e| {
                    ExtractTaskError::Message(format!("Failed to read entries: {e}"))
                })?;

                extract_entries_with_progress(
                    &mut *reader,
                    &entries,
                    &out_dir,
                    overwrite,
                    &cancel_token,
                    &emitter,
                )
            })();

            match task_result {
                Ok(result) => {
                    let (current, completed_entries) = emitter.latest_snapshot();
                    emitter.emit_finalizing(current, completed_entries);
                    emitter.emit_finished(current, completed_entries);
                    Ok(result)
                }
                Err(ExtractTaskError::Cancelled) => {
                    let (current, completed_entries) = emitter.latest_snapshot();
                    emitter.emit_cancelled(current, completed_entries);
                    Err(CANCELLED_MESSAGE.to_string())
                }
                Err(ExtractTaskError::Message(message)) => {
                    let (current, completed_entries) = emitter.latest_snapshot();
                    emitter.emit_failed(current, completed_entries, message.clone());
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
// Shared extraction helpers
// ---------------------------------------------------------------------------

pub(crate) fn extract_entries_with_progress(
    reader: &mut dyn ArchiveReader,
    entries: &[Entry],
    dest: &Path,
    overwrite: bool,
    cancel_token: &Arc<AtomicBool>,
    emitter: &TaskProgressEmitter,
) -> Result<ExtractArchiveResult, ExtractTaskError> {
    let dest = normalize_path_for_extract(dest);
    fs::create_dir_all(&dest).map_err(|e| {
        ExtractTaskError::Message(format!(
            "Cannot create output directory '{}': {}",
            dest.display(),
            e
        ))
    })?;

    let total_bytes = entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .map(|entry| entry.size)
        .sum::<u64>();
    let total_entries = entries.len() as u64;

    emitter.set_totals(Some(total_bytes), Some(total_entries));
    emitter.emit_progress(TaskStage::Extracting, None, 0, None, 0, true);

    let mut report = ExtractReport::default();
    let mut progress_bytes = 0u64;
    let mut completed_entries = 0u64;

    for entry in entries {
        if cancel_token.load(Ordering::SeqCst) {
            return Err(ExtractTaskError::Cancelled);
        }

        let entry_path = Path::new(&entry.path);
        let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
            Ok(target) => target,
            Err((name, err)) => {
                report.errors.push((name, err));
                progress_bytes = progress_bytes.saturating_add(entry.size);
                completed_entries += 1;
                emitter.emit_progress(
                    TaskStage::Extracting,
                    None,
                    progress_bytes,
                    Some(&entry.path),
                    completed_entries,
                    true,
                );
                continue;
            }
        };

        if entry.is_dir {
            if let Err(e) = fs::create_dir_all(&target) {
                report.errors.push((
                    entry.path.clone(),
                    GeeZipError::io(e, "Cannot create directory"),
                ));
            } else {
                report.files_extracted += 1;
            }
            completed_entries += 1;
            emitter.emit_progress(
                TaskStage::Extracting,
                None,
                progress_bytes,
                Some(&entry.path),
                completed_entries,
                true,
            );
            continue;
        }

        if let Some(parent) = target.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    report.errors.push((
                        entry.path.clone(),
                        GeeZipError::io(e, "Cannot create parent directory"),
                    ));
                    progress_bytes = progress_bytes.saturating_add(entry.size);
                    completed_entries += 1;
                    emitter.emit_progress(
                        TaskStage::Extracting,
                        None,
                        progress_bytes,
                        Some(&entry.path),
                        completed_entries,
                        true,
                    );
                    continue;
                }
            }
        }

        let output = if overwrite {
            match fs::File::create(&target) {
                Ok(file) => file,
                Err(e) => {
                    report.errors.push((
                        entry.path.clone(),
                        GeeZipError::io(e, "Cannot create output file"),
                    ));
                    progress_bytes = progress_bytes.saturating_add(entry.size);
                    completed_entries += 1;
                    emitter.emit_progress(
                        TaskStage::Extracting,
                        None,
                        progress_bytes,
                        Some(&entry.path),
                        completed_entries,
                        true,
                    );
                    continue;
                }
            }
        } else {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    report.files_skipped += 1;
                    report.errors.push((
                        entry.path.clone(),
                        GeeZipError::clobber_denied(entry.path.clone()),
                    ));
                    progress_bytes = progress_bytes.saturating_add(entry.size);
                    completed_entries += 1;
                    emitter.emit_progress(
                        TaskStage::Extracting,
                        None,
                        progress_bytes,
                        Some(&entry.path),
                        completed_entries,
                        true,
                    );
                    continue;
                }
                Err(e) => {
                    report.errors.push((
                        entry.path.clone(),
                        GeeZipError::io(e, "Cannot create output file"),
                    ));
                    progress_bytes = progress_bytes.saturating_add(entry.size);
                    completed_entries += 1;
                    emitter.emit_progress(
                        TaskStage::Extracting,
                        None,
                        progress_bytes,
                        Some(&entry.path),
                        completed_entries,
                        true,
                    );
                    continue;
                }
            }
        };

        let callback = emitter.writer_callback(
            cancel_token.clone(),
            TaskStage::Extracting,
            progress_bytes,
            entry.path.clone(),
            completed_entries,
        );
        let mut output = ProgressWriter::new(output)
            .with_total(entry.size)
            .with_callback(Box::new(callback));

        match reader.extract(entry, &mut output) {
            Ok(bytes) => {
                report.files_extracted += 1;
                report.bytes_extracted += bytes;
            }
            Err(err) => {
                if cancel_token.load(Ordering::SeqCst) || is_cancelled_error(&err) {
                    return Err(ExtractTaskError::Cancelled);
                }
                report.errors.push((entry.path.clone(), err));
            }
        }

        progress_bytes = progress_bytes.saturating_add(entry.size);
        completed_entries += 1;
        emitter.emit_progress(
            TaskStage::Extracting,
            None,
            progress_bytes,
            Some(&entry.path),
            completed_entries,
            true,
        );
    }

    Ok(ExtractArchiveResult::from(report))
}

/// Normalise a path for use as an extraction destination.
///
/// If the path exists, uses `canonicalize` for a true absolute path. If it does
/// not exist, still normalises `.` and `..` components to avoid confusing path
/// joins while preserving the zip-slip check.
pub(crate) fn normalize_path_for_extract(path: &Path) -> PathBuf {
    if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else {
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::Normal(_)
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_) => {
                    result.push(component);
                }
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    result.pop();
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_non_existent_clean_path() {
        let p = normalize_path_for_extract(Path::new("/tmp/geezipx-test/does-not-exist"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/does-not-exist"));
    }

    #[test]
    fn normalize_non_existent_with_curdir() {
        let p = normalize_path_for_extract(Path::new("/tmp/geezipx-test/./subdir/././file.txt"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/subdir/file.txt"));
    }

    #[test]
    fn normalize_non_existent_with_parentdir() {
        let p = normalize_path_for_extract(Path::new("/tmp/geezipx-test/subdir/../file.txt"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/file.txt"));
    }

    #[test]
    fn normalize_non_existent_mixed_curdir_parentdir() {
        let p =
            normalize_path_for_extract(Path::new("/tmp/geezipx-test/./subdir/.././other/deeper"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/other/deeper"));
    }

    #[test]
    fn normalize_non_existent_relative_with_parentdir() {
        let p = normalize_path_for_extract(Path::new("foo/../../bar"));
        assert_eq!(p, Path::new("bar"));
    }

    #[test]
    fn normalize_existing_path_canonicalizes() {
        let dir = std::env::temp_dir().join("geezipx-test-normalize");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let sub = dir.join("sub");
        fs::create_dir(&sub).expect("create sub dir");

        let p = normalize_path_for_extract(&sub);
        assert!(p.is_absolute());
        assert!(p.ends_with("geezipx-test-normalize/sub"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_non_existent_with_multiple_parentdir() {
        let p = normalize_path_for_extract(Path::new("a/b/c/../../../d"));
        assert_eq!(p, Path::new("d"));
    }

    #[test]
    fn normalize_empty_path() {
        let p = normalize_path_for_extract(Path::new(""));
        assert_eq!(p, Path::new(""));
    }
}
