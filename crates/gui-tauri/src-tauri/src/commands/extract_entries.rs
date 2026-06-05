//! `extract_entries` command — selectively extract entries from an archive.
//!
//! Extracts a subset of entries (files or directories) from an archive to a
//! target directory.  Reuses the shared helpers from the `list` module and
//! core `ArchiveReader::extract()` for per-entry extraction.
//!
//! ## Supported formats
//!
//! Same as `extract_archive`: zip, tar, tar.gz, tar.zst, tar.xz, 7z, rar.
//! Single-stream formats return a clear error.
//!
//! ## Cancellation
//!
//! Supports the same cancellation token scheme as `extract_archive`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::task::spawn_blocking;

use geezipx_core::archive::check_entry_path_safety;
use geezipx_core::archive::Entry;
use geezipx_core::archive::ExtractReport;
use geezipx_core::detect::ArchiveFormat;
use geezipx_core::GeeZipError;

use crate::commands::extract::{ExtractArchiveResult, ExtractErrorInfo};
use crate::commands::list::{detect_archive_format, open_reader};
use crate::state::AppState;

/// Selectively extract entries from an archive.
///
/// `entry_paths` limits extraction to specific entries.  If an entry is a
/// directory, all its descendants are extracted as well.
///
/// ## Cancellation
///
/// When `task_id` is provided a cancellation token is registered.  Call
/// `cancel_task` with the same id to abort.
#[tauri::command]
pub async fn extract_entries(
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
        let format = detect_archive_format(&path_buf)?;

        // Single-stream formats: reject.
        match format {
            ArchiveFormat::Gzip | ArchiveFormat::Zstd | ArchiveFormat::Xz | ArchiveFormat::Lzma => {
                return Err(format!(
                    "'{format}' is a single-stream compression format; \
                     selective extraction is not supported (use full extraction)"
                ));
            }
            _ => {}
        }

        let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;
        let all_entries = reader
            .entries()
            .map_err(|e| format!("Failed to read entries: {e}"))?;

        // Build a set of entry paths to extract.  Normalise each requested
        // path and also match descendants if a directory is requested.
        let requested_set: HashSet<String> = entry_paths.iter().cloned().collect();
        let mut matched: Vec<Entry> = Vec::new();

        for entry in &all_entries {
            // Check cancellation before each entry check.
            if cancel_token.load(Ordering::SeqCst) {
                return Err("Operation cancelled by user".to_string());
            }

            if requested_set.contains(&entry.path) {
                matched.push(entry.clone());
                continue;
            }
            // If a requested entry is a directory, match all descendants.
            for req in &entry_paths {
                if entry.path.starts_with(req) {
                    let after_prefix = &entry.path[req.len()..];
                    // Must either be empty (exact match, already caught above)
                    // or start with '/' (child entry).
                    if after_prefix.is_empty() || after_prefix.starts_with('/') {
                        matched.push(entry.clone());
                        break;
                    }
                }
            }
        }

        if matched.is_empty() {
            return Err(
                "No matching entries found in archive for the requested path(s)".to_string(),
            );
        }

        // Normalise destination path.
        let dest = normalize_path_for_extract(&out_dir);
        // Ensure output directory exists.
        fs::create_dir_all(&dest)
            .map_err(|e| format!("Cannot create output directory '{}': {}", dest.display(), e))?;

        let mut report = ExtractReport::default();

        for entry in &matched {
            // Check cancellation before each entry.
            if cancel_token.load(Ordering::SeqCst) {
                return Err("Operation cancelled by user".to_string());
            }

            let entry_path = Path::new(&entry.path);

            let target = match check_entry_path_safety(entry_path, &entry.path, &dest) {
                Ok(t) => t,
                Err((name, err)) => {
                    report.errors.push((name, err));
                    continue;
                }
            };

            // Handle directory entries.
            if entry.is_dir {
                if let Err(e) = fs::create_dir_all(&target) {
                    report.errors.push((
                        entry.path.clone(),
                        GeeZipError::io(e, "Cannot create directory"),
                    ));
                } else {
                    report.files_extracted += 1;
                }
                continue;
            }

            // Create parent directory.
            if let Some(parent) = target.parent() {
                if !parent.exists() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, "Cannot create parent directory"),
                        ));
                        continue;
                    }
                }
            }

            // Open output file.
            let mut output = if overwrite {
                match fs::File::create(&target) {
                    Ok(f) => f,
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, "Cannot create output file"),
                        ));
                        continue;
                    }
                }
            } else {
                match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                {
                    Ok(f) => f,
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        report.files_skipped += 1;
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::clobber_denied(entry.path.clone()),
                        ));
                        continue;
                    }
                    Err(e) => {
                        report.errors.push((
                            entry.path.clone(),
                            GeeZipError::io(e, "Cannot create output file"),
                        ));
                        continue;
                    }
                }
            };

            // Extract entry content.
            match reader.extract(entry, &mut output) {
                Ok(bytes) => {
                    report.files_extracted += 1;
                    report.bytes_extracted += bytes;
                }
                Err(e) => {
                    report.errors.push((entry.path.clone(), e));
                }
            }
        }

        Ok(ExtractArchiveResult {
            files_extracted: report.files_extracted as u64,
            bytes_extracted: report.bytes_extracted,
            files_skipped: report.files_skipped as u64,
            errors: report
                .errors
                .into_iter()
                .map(|(path, msg)| ExtractErrorInfo {
                    path,
                    message: msg.to_string(),
                })
                .collect(),
        })
    })
    .await;

    // --- Clean up cancellation token ---
    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    tokens.remove(&tid);
    drop(tokens);

    let result = result.map_err(|e| format!("Internal error: {e}"))?;
    result
}

/// Normalise a path for use as an extraction destination.
///
/// If the path exists, uses `canonicalize` for a true absolute path.
/// If it does not exist, still normalises `.` and `..` components to avoid
/// confusing path joins while preserving the zip-slip check.
fn normalize_path_for_extract(path: &Path) -> PathBuf {
    if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else {
        // Even when the path doesn't exist, normalise '.' and '..' components
        // so the caller gets a clean path for comparisons and joins.
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::Normal(_)
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_) => {
                    result.push(component);
                }
                std::path::Component::CurDir => {
                    // Skip '.' (current directory) — it's a no-op.
                }
                std::path::Component::ParentDir => {
                    // Pop the last component; silently ignore attempts to go
                    // above the filesystem root.
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
    use std::path::Path;

    #[test]
    fn normalize_non_existent_clean_path() {
        // A non-existent path without '.' or '..' should be preserved as-is.
        let p = normalize_path_for_extract(Path::new("/tmp/geezipx-test/does-not-exist"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/does-not-exist"));
    }

    #[test]
    fn normalize_non_existent_with_curdir() {
        // '.' components in a non-existent path should be stripped.
        let p = normalize_path_for_extract(Path::new("/tmp/geezipx-test/./subdir/././file.txt"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/subdir/file.txt"));
    }

    #[test]
    fn normalize_non_existent_with_parentdir() {
        // '..' components in a non-existent path should be normalised away.
        let p = normalize_path_for_extract(Path::new("/tmp/geezipx-test/subdir/../file.txt"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/file.txt"));
    }

    #[test]
    fn normalize_non_existent_mixed_curdir_parentdir() {
        // Mixed '.' and '..' components.
        let p =
            normalize_path_for_extract(Path::new("/tmp/geezipx-test/./subdir/.././other/deeper"));
        assert_eq!(p, Path::new("/tmp/geezipx-test/other/deeper"));
    }

    #[test]
    fn normalize_non_existent_relative_with_parentdir() {
        // Relative paths with '..' going beyond root are silently handled.
        let p = normalize_path_for_extract(Path::new("foo/../../bar"));
        assert_eq!(p, Path::new("bar"));
    }

    #[test]
    fn normalize_existing_path_canonicalizes() {
        use std::fs;
        let dir = std::env::temp_dir().join("geezipx-test-normalize");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let sub = dir.join("sub");
        fs::create_dir(&sub).expect("create sub dir");
        // Canonicalize should resolve the path.
        let p = normalize_path_for_extract(&sub);
        assert!(p.is_absolute());
        assert!(p.ends_with("geezipx-test-normalize/sub"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_non_existent_with_multiple_parentdir() {
        // Multiple '..' segments.
        let p = normalize_path_for_extract(Path::new("a/b/c/../../../d"));
        assert_eq!(p, Path::new("d"));
    }

    #[test]
    fn normalize_empty_path() {
        // Empty path stays empty.
        let p = normalize_path_for_extract(Path::new(""));
        assert_eq!(p, Path::new(""));
    }
}
