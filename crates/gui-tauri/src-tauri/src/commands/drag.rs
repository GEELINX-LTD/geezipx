//! `drag` commands — prepare drag-out temp files and clean up.
//!
//! To support dragging entries out of the archive browser, we first extract
//! selected entries to a GeeZipX‑specific temporary directory, then invoke
//! the [`tauri-plugin-drag`] to start a system drag of those real files.
//!
//! ## Temp directory layout
//!
//! ```text
//! <platform temp>/geezipx-dragout/<temp_id>/<entry files/dirs...>
//! ```
//!
//! ## Cleanup strategy
//!
//! The frontend should call [`cleanup_drag_temp_dir`] when the drag completes
//! (either dropped or cancelled).  A periodic background task can also call
//! [`cleanup_stale_drag_temp_dirs`] to reclaim disk space from abandoned
//! drag operations.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::commands::list::{detect_archive_format, open_reader};

/// Top‑level directory name for drag‑out temp files.
const DRAG_TEMP_ROOT: &str = "geezipx-dragout";

/// Maximum age for a drag‑out temp directory before it is considered stale
/// and eligible for cleanup.
const STALE_AGE: Duration = Duration::from_secs(3600); // 1 hour

// ---------------------------------------------------------------------------
// Public commands
// ---------------------------------------------------------------------------

/// Extract a set of archive entries to a temporary directory so they can be
/// dragged out of the GeeZipX window.
///
/// Returns the absolute path to the temp directory containing the extracted
/// files, or an error if the extraction fails.
#[tauri::command]
pub async fn prepare_drag_entries(
    archive_path: String,
    entry_paths: Vec<String>,
    password: Option<String>,
) -> Result<String, String> {
    if entry_paths.is_empty() {
        return Err("At least one entry path is required".to_string());
    }

    let path_buf = PathBuf::from(&archive_path);
    let pwd = password;

    // Build a temp id from the first entry path so repeated drags of the
    // same entries can be cached.
    let temp_id = short_id(&entry_paths[0]);
    let temp_root = temp_dir_root();
    let dest = temp_root.join(&temp_id);

    // If the directory already exists, assume previous extraction is still
    // valid (the frontend manages cleanup).  This avoids re‑extracting when
    // the user starts a drag, cancels, and immediately starts again.
    if dest.exists() {
        return Ok(dest.to_string_lossy().to_string());
    }

    // Clone dest before the move closure so we can clean up on errors.
    let dest_for_cleanup = dest.clone();

    // --- Run extraction on the blocking pool ---
    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let format = detect_archive_format(&path_buf)?;
        let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;
        let all_entries = reader
            .entries()
            .map_err(|e| format!("Failed to read archive entries: {e}"))?;

        // Ensure the destination directory exists.
        fs::create_dir_all(&dest).map_err(|e| format!("Cannot create temp directory: {e}"))?;

        // Collect the requested entries.
        let requested: std::collections::HashSet<&str> =
            entry_paths.iter().map(|s| s.as_str()).collect();

        // Track which directory entries we've already created so we don't
        // attempt to create the same directory twice.
        let mut created_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for entry in &all_entries {
            // If this entry path (or a prefix) was requested, extract it.
            let should_extract = requested.contains(entry.path.as_str())
                || entry_paths.iter().any(|req| {
                    entry.path.starts_with(req)
                        && (entry.path.len() == req.len()
                            || entry.path[req.len()..].starts_with('/'))
                });

            if !should_extract {
                continue;
            }

            let target = dest.join(entry.path.trim_start_matches('/'));

            if entry.is_dir {
                if created_dirs.insert(entry.path.clone()) {
                    fs::create_dir_all(&target)
                        .map_err(|e| format!("Cannot create dir '{}': {e}", target.display()))?;
                }
            } else {
                // Ensure parent directory exists.
                if let Some(parent) = target.parent() {
                    let parent_str = parent.to_string_lossy().to_string();
                    if created_dirs.insert(parent_str) {
                        fs::create_dir_all(parent).map_err(|e| {
                            format!("Cannot create parent dir '{}': {e}", parent.display())
                        })?;
                    }
                }

                let mut output = fs::File::create(&target)
                    .map_err(|e| format!("Cannot create temp file '{}': {e}", target.display()))?;

                reader
                    .extract(entry, &mut output)
                    .map_err(|e| format!("Extraction error: {e}"))?;
            }
        }

        Ok(dest.to_string_lossy().to_string())
    })
    .await;

    match result {
        Ok(Ok(path)) => Ok(path),
        Ok(Err(e)) => {
            // Clean up partial extraction on error.
            let _ = fs::remove_dir_all(&dest_for_cleanup);
            Err(e)
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&dest_for_cleanup);
            Err(format!("Internal error: {e}"))
        }
    }
}

/// Remove a drag‑out temp directory identified by `temp_id`.
#[tauri::command]
pub async fn cleanup_drag_temp_dir(temp_id: String) -> Result<(), String> {
    let dest = temp_dir_root().join(&temp_id);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("Cleanup failed: {e}"))?;
    }
    Ok(())
}

/// Scan the drag‑out temp root and remove directories older than the staleness
/// threshold.  Returns the number of cleaned directories.
#[tauri::command]
pub async fn cleanup_stale_drag_temp_dirs() -> Result<u32, String> {
    let root = temp_dir_root();
    if !root.exists() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut cleaned = 0u32;

    let entries = fs::read_dir(&root).map_err(|e| format!("Cannot read drag temp root: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Read dir error: {e}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Check staleness by modification time.
        let modified = path
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);

        if now.duration_since(modified).unwrap_or(Duration::ZERO) > STALE_AGE {
            let _ = fs::remove_dir_all(&path);
            cleaned += 1;
        }
    }

    Ok(cleaned)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the platform‑appropriate temp directory root for drag‑out staging.
fn temp_dir_root() -> PathBuf {
    std::env::temp_dir().join(DRAG_TEMP_ROOT)
}

/// Derive a short, filesystem‑safe identifier from an entry path.
///
/// Uses a std hash of the path to produce a compact hex string suitable
/// for use as a temp directory name component.
fn short_id(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();
    // Format as 16 hex chars (u64 = up to 16 hex digits).
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_id_is_deterministic() {
        let a = short_id("/some/archive/path/file.txt");
        let b = short_id("/some/archive/path/file.txt");
        assert_eq!(a, b);
    }

    #[test]
    fn short_id_differs_for_diff_paths() {
        let a = short_id("/path/a/file.txt");
        let b = short_id("/path/b/file.txt");
        assert_ne!(a, b);
    }

    #[test]
    fn short_id_is_filesystem_safe() {
        let id = short_id("/some/weird/path/with spaces/& special?.txt");
        // Should be lowercase hex characters only.
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn short_id_length_is_always_16() {
        for input in &[
            "",
            "a",
            "short",
            "a/very/long/path/that/exceeds/typical/lengths.txt",
        ] {
            assert_eq!(short_id(input).len(), 16);
        }
    }

    #[test]
    fn short_id_handles_empty_string() {
        let id = short_id("");
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn temp_dir_root_is_absolute() {
        let root = temp_dir_root();
        assert!(root.is_absolute());
        assert!(root.ends_with(DRAG_TEMP_ROOT));
    }
}
