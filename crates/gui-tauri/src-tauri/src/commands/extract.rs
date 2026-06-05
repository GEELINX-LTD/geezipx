//! `extract_archive` command — extract all entries from an archive.
//!
//! Reuses the shared helpers from the `list` module (`detect_archive_format`,
//! `open_reader`) and registers a cancellation token via `AppState`.
//!
//! # Single-stream formats (gzip, zstd, xz, lzma)
//!
//! These are not yet supported for extraction in the GUI.  The command returns
//! a clear error directing users to a future update.  Do **not** route them
//! through `open_reader`, which already rejects them with a list-specific
//! error message.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tokio::task::spawn_blocking;

use geezipx_core::archive::ExtractReport;
use geezipx_core::detect::ArchiveFormat;

use crate::commands::list::{detect_archive_format, open_reader};
use crate::state::AppState;

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

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Extract all entries from an archive to a directory.
///
/// ## Supported formats
///
/// - Archive-based: zip, tar, tar.gz, tar.zst, tar.xz, 7z, rar.
/// - Single-stream (gzip, zstd, xz, lzma): returns a clear error — the GUI
///   will support single-stream decompression in a later update.
///
/// ## Cancellation
///
/// If `task_id` is provided the command registers a cancellation token in
/// [`AppState::cancel_tokens`].  The frontend can call `cancel_task` with the
/// same id to abort the extraction.  The token is always cleaned up after the
/// command completes (success, error, or cancellation).
#[tauri::command]
pub async fn extract_archive(
    state: tauri::State<'_, AppState>,
    archive_path: String,
    output_dir: String,
    overwrite: bool,
    password: Option<String>,
    task_id: Option<String>,
) -> Result<ExtractArchiveResult, String> {
    // Generate a task id if not provided by the frontend.
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

    // --- Run extraction on the blocking pool ---
    let result = spawn_blocking(move || {
        // 1. Detect format.
        let format = detect_archive_format(&path_buf)?;

        // 2. Single-stream formats: not yet supported for extraction in the GUI.
        match format {
            ArchiveFormat::Gzip | ArchiveFormat::Zstd | ArchiveFormat::Xz | ArchiveFormat::Lzma => {
                return Err(format!(
                    "'{format}' is a single-stream compression format; \
                     single-stream decompression is not yet supported in the GUI \
                     (will be added in a later update)"
                ));
            }
            _ => {}
        }

        // 3. Open archive reader.
        let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;

        // 4. Extract with cancellation.
        let report = reader
            .extract_all_with_cancel(&out_dir, overwrite, &|| cancel_token.load(Ordering::SeqCst))
            .map_err(|e| format!("Extraction failed: {e}"))?;

        // 5. Map to serialisable result.
        Ok(ExtractArchiveResult::from(report))
    })
    .await;

    // --- Clean up cancellation token ---
    // Always run cleanup after the blocking task finishes, even if the task
    // panicked and `.await` returned a JoinError.
    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    tokens.remove(&tid);
    drop(tokens);

    let result = result.map_err(|e| format!("Internal error: {e}"))?;
    result
}
