//! `drag` commands — prepare drag-out temp files and clean up.
//!
//! To support dragging entries out of the archive browser, we first extract
//! selected entries to a GeeZipX‑specific temporary directory, then invoke
//! the [`tauri-plugin-drag`] to start a system drag of those real files.
//!
//! ## Temp directory layout
//!
//! ```text
//! <platform temp>/geezipx-dragout/<archive-name>/<entry files/dirs...>
//! ```
//!
//! ## Cleanup strategy
//!
//! The frontend should call [`cleanup_drag_temp_dir`] when the drag completes
//! (either dropped or cancelled).  A periodic background task can also call
//! [`cleanup_stale_drag_temp_dirs`] to reclaim disk space from abandoned
//! drag operations.

use std::fs;
use std::path::{Path, PathBuf};
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

    let dir_name = derive_drag_directory_name(&path_buf);
    let temp_root = temp_dir_root();
    let dest = temp_root.join(&dir_name);

    // Clone dest before the move closure so we can clean up on errors.
    let dest_for_cleanup = dest.clone();

    // --- Run extraction on the blocking pool ---
    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let format = detect_archive_format(&path_buf)?;
        let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;
        let all_entries = reader
            .entries()
            .map_err(|e| format!("Failed to read archive entries: {e}"))?;

        if dest.exists() {
            fs::remove_dir_all(&dest)
                .map_err(|e| format!("Cannot replace temp directory '{}': {e}", dest.display()))?;
        }

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

/// Remove a drag‑out temp directory identified by its directory name.
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

/// Derive a drag‑out directory name from the archive path.
fn derive_drag_directory_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive");

    sanitize_dir_name(strip_archive_extension(file_name))
}

fn strip_archive_extension(file_name: &str) -> &str {
    const COMPOUND_EXTENSIONS: &[&str] = &[
        ".tar.lzma",
        ".tar.bz2",
        ".tar.zst",
        ".tar.gz",
        ".tar.xz",
        ".tar.br",
        ".tar.z",
        ".tgz",
        ".tbz2",
        ".txz",
        ".tzst",
    ];
    const SIMPLE_EXTENSIONS: &[&str] = &[
        ".zip", ".rar", ".7z", ".tar", ".gz", ".zst", ".xz", ".lzma", ".bz2", ".br", ".z",
    ];

    let lower = file_name.to_ascii_lowercase();

    for ext in COMPOUND_EXTENSIONS {
        if lower.ends_with(ext) {
            return &file_name[..file_name.len() - ext.len()];
        }
    }

    for ext in SIMPLE_EXTENSIONS {
        if lower.ends_with(ext) {
            return &file_name[..file_name.len() - ext.len()];
        }
    }

    file_name
}

fn sanitize_dir_name(name: &str) -> String {
    const MAX_DIR_NAME_CHARS: usize = 100;

    fn trim_edge_dots_and_spaces(value: &str) -> &str {
        value.trim_matches(|ch: char| ch == '.' || ch.is_whitespace())
    }

    fn truncate_chars(value: &str, max_chars: usize) -> &str {
        let end = value
            .char_indices()
            .map(|(idx, _)| idx)
            .nth(max_chars)
            .unwrap_or(value.len());
        &value[..end]
    }

    fn finalize(value: &str) -> Option<String> {
        let trimmed = trim_edge_dots_and_spaces(value);
        let truncated = truncate_chars(trimmed, MAX_DIR_NAME_CHARS);
        let cleaned = trim_edge_dots_and_spaces(truncated);
        (!cleaned.is_empty()).then(|| cleaned.to_string())
    }

    fn is_windows_reserved_base_name(name: &str) -> bool {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "con"
                | "prn"
                | "aux"
                | "nul"
                | "com1"
                | "com2"
                | "com3"
                | "com4"
                | "com5"
                | "com6"
                | "com7"
                | "com8"
                | "com9"
                | "lpt1"
                | "lpt2"
                | "lpt3"
                | "lpt4"
                | "lpt5"
                | "lpt6"
                | "lpt7"
                | "lpt8"
                | "lpt9"
        )
    }

    let normalized: String = name
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();

    let mut result = finalize(&normalized).unwrap_or_else(|| "archive".to_string());
    let base = result.split('.').next().unwrap_or_default();
    if is_windows_reserved_base_name(base) {
        result.insert(0, '_');
        result = finalize(&result).unwrap_or_else(|| "archive".to_string());
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_drag_directory_name_uses_archive_stem_for_supported_formats() {
        for archive_name in [
            "archive.zip",
            "archive.tar.gz",
            "archive.tar.bz2",
            "archive.tar.xz",
            "archive.tar.zst",
            "archive.tar.lzma",
            "archive.tgz",
            "archive.tbz2",
        ] {
            assert_eq!(
                derive_drag_directory_name(Path::new(archive_name)),
                "archive",
                "{archive_name}"
            );
        }
    }

    #[test]
    fn strip_archive_extension_supports_case_insensitive_tar_variants() {
        assert_eq!(strip_archive_extension("ARCHIVE.TAR.GZ"), "ARCHIVE");
        assert_eq!(strip_archive_extension("archive.tar.br"), "archive");
        assert_eq!(strip_archive_extension("archive.tar.Z"), "archive");
        assert_eq!(strip_archive_extension("archive.tar.z"), "archive");
    }

    #[test]
    fn derive_drag_directory_name_preserves_chinese_names() {
        assert_eq!(
            derive_drag_directory_name(Path::new("中文备份.zip")),
            "中文备份"
        );
    }

    #[test]
    fn sanitize_dir_name_replaces_invalid_characters() {
        assert_eq!(sanitize_dir_name("bad<na>me:\u{7}?*"), "bad_na_me____");
        assert_eq!(sanitize_dir_name("中?文<备>份"), "中_文_备_份");
    }

    #[test]
    fn sanitize_dir_name_cleans_leading_and_trailing_dots_and_spaces() {
        assert_eq!(sanitize_dir_name(".foo"), "foo");
        assert_eq!(sanitize_dir_name("foo."), "foo");
        assert_eq!(sanitize_dir_name("  .foo.  "), "foo");
        assert_eq!(derive_drag_directory_name(Path::new(".zip")), "archive");
        assert_eq!(derive_drag_directory_name(Path::new("..zip")), "archive");
    }

    #[test]
    fn derive_drag_directory_name_truncates_unicode_without_panicking() {
        let archive_name = format!("{}.zip", "测".repeat(120));
        let result = derive_drag_directory_name(Path::new(&archive_name));

        assert_eq!(result.chars().count(), 100);
        assert!(result.chars().all(|ch| ch == '测'));
    }

    #[test]
    fn sanitize_dir_name_cleans_trailing_dot_and_space_after_truncation() {
        let result = sanitize_dir_name(&("a".repeat(98) + " ."));

        assert_eq!(result, "a".repeat(98));
        assert!(!result.ends_with('.'));
        assert!(!result.ends_with(' '));
    }

    #[test]
    fn derive_drag_directory_name_prefixes_windows_reserved_names_with_extensions() {
        assert_eq!(
            derive_drag_directory_name(Path::new("CON.backup.zip")),
            "_CON.backup"
        );
        assert_eq!(
            derive_drag_directory_name(Path::new("lpt1.v1.zip")),
            "_lpt1.v1"
        );
    }

    #[test]
    fn temp_dir_root_is_absolute() {
        let root = temp_dir_root();
        assert!(root.is_absolute());
        assert!(root.ends_with(DRAG_TEMP_ROOT));
    }
}
