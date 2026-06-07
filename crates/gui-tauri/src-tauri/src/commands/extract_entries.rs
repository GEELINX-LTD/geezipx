//! `extract_entries` command — selectively extract entries from an archive.
//!
//! Extracts a subset of entries (files or directories) from an archive to a
//! target directory. Progress events are emitted through the shared extraction
//! helper used by `extract_archive`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;
use tokio::task::spawn_blocking;

use geezipx_core::archive::Entry;
use geezipx_core::detect::ArchiveFormat;

use crate::commands::extract::{
    extract_entries_with_progress, ExtractArchiveResult, ExtractTaskError,
};
use crate::commands::list::{detect_archive_format, open_reader};
use crate::commands::progress::{TaskKind, TaskProgressEmitter};
use crate::state::AppState;

const CANCELLED_MESSAGE: &str = "Operation cancelled by user";

/// Selectively extract entries from an archive.
///
/// `entry_paths` limits extraction to specific entries. If an entry is a
/// directory, all its descendants are extracted as well.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn extract_entries(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    archive_path: String,
    entry_paths: Vec<String>,
    output_dir: String,
    overwrite: bool,
    password: Option<String>,
    task_id: Option<String>,
) -> Result<ExtractArchiveResult, String> {
    if entry_paths.is_empty() {
        return Err("At least one entry path is required".to_string());
    }

    let tid = task_id.unwrap_or_else(|| {
        format!(
            "extract-entries-{}",
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
                    | ArchiveFormat::Bzip2
                    | ArchiveFormat::Zstd
                    | ArchiveFormat::Xz
                    | ArchiveFormat::Lzma => {
                        return Err(ExtractTaskError::Message(format!(
                            "'{format}' is a single-stream compression format; \
                             selective extraction is not supported (use full extraction)"
                        )));
                    }
                    _ => {}
                }

                let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;
                let all_entries = reader.entries().map_err(|e| {
                    ExtractTaskError::Message(format!("Failed to read entries: {e}"))
                })?;

                let matched = select_requested_entries(&all_entries, &entry_paths, &cancel_token)?;
                if matched.is_empty() {
                    return Err(ExtractTaskError::Message(
                        "No matching entries found in archive for the requested path(s)"
                            .to_string(),
                    ));
                }

                extract_entries_with_progress(
                    &mut *reader,
                    &matched,
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

fn select_requested_entries(
    all_entries: &[Entry],
    entry_paths: &[String],
    cancel_token: &Arc<AtomicBool>,
) -> Result<Vec<Entry>, ExtractTaskError> {
    let requested_set: HashSet<String> = entry_paths.iter().cloned().collect();
    let mut matched = Vec::new();

    for entry in all_entries {
        if cancel_token.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ExtractTaskError::Cancelled);
        }

        if requested_set.contains(&entry.path) {
            matched.push(entry.clone());
            continue;
        }

        for requested in entry_paths {
            if entry.path.starts_with(requested) {
                let suffix = &entry.path[requested.len()..];
                if suffix.is_empty() || suffix.starts_with('/') {
                    matched.push(entry.clone());
                    break;
                }
            }
        }
    }

    Ok(matched)
}
