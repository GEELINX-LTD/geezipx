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
