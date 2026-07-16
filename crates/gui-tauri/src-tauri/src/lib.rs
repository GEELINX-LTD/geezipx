//! GeeZipX GUI — Tauri v2 application backend.
//!
//! This crate is a thin bridge between the Tauri frontend and `geezipx-core`.
//! No compression/decompression logic lives here.

use serde::Serialize;
use tauri::Emitter;
use tauri::Manager;

pub(crate) mod com_server;
pub mod commands;
pub(crate) mod shell_action_file;
pub mod state;

// Re-export for main.rs (platform-independent embedding detection).
pub use com_server::is_embedding_arg;

// Re-exports for Phase B COM server + Phase C (shell_menu.rs) — Windows only.
#[cfg(target_os = "windows")]
pub use com_server::{action_for_clsid, run_com_server, CLSID_COMPRESS, CLSID_COMPRESS_ZIP};

// ---------------------------------------------------------------------------
// Shell context menu action types
// ---------------------------------------------------------------------------

/// Payload sent to the frontend via `shell-action` event.
#[derive(Debug, Clone, Serialize)]
pub struct ShellActionPayload {
    /// One of "extract", "extract-here", "compress".
    pub action: String,
    /// Archive format for compress action (e.g. "zip", "7z").
    pub format: Option<String>,
    /// File/directory paths passed from the shell.
    pub paths: Vec<String>,
}

/// Parsed shell context-menu action from command-line args.
#[derive(Debug)]
enum ShellAction {
    /// Legacy file association: no explicit flag, treat args as archive paths.
    OpenArchives(Vec<String>),
    /// "用 GeeZipX 打开" — browse archive.
    Open(Vec<String>),
    /// "解压缩到..." — jump to extract page.
    Extract(Vec<String>),
    /// "解压缩到当前文件夹" — smart extract.
    ExtractHere(Vec<String>),
    /// "压缩为 ZIP" — headless quick ZIP compress.
    CompressZip(Vec<String>),
    /// "压缩为..." — jump to compress page.
    Compress(Vec<String>),
}

/// Result from the unified shell argument resolver.
#[derive(Debug)]
enum ResolvedAction {
    /// An explicit shell action with payload (ready to emit).
    Action(ShellActionPayload),
    /// Legacy open-archives — paths without an explicit flag.
    OpenArchives(Vec<String>),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Unified shell argument resolver — the single entry point for both cold
/// start and single-instance callbacks.
///
/// Resolution order:
/// 1. `--shell-action-file <path>` — reads a versioned binary action file
///    written by the DelegateExecute COM handler (multi-select support).
/// 2. Legacy CLI flags (`/extract`, `/compress`, etc.) with `"%1"` paths.
/// 3. Bare paths — treated as legacy "open archives".
///
/// Returns `None` when no actionable paths were found (e.g. the `%*`
/// placeholder was passed literally).
fn resolve_shell_action(args: &[String]) -> Option<ResolvedAction> {
    // --- Priority 1: --shell-action-file ----------------------------------
    if let Some(file_path) = extract_flag_value(args, "--shell-action-file") {
        match read_action_file_action(file_path) {
            Ok(payload) => return Some(ResolvedAction::Action(payload)),
            Err(e) => {
                eprintln!("shell-action-file error ({}): {e}", file_path);
                // Don't fall through — a broken action file is an explicit
                // intent; returning None avoids misinterpreting the path as
                // a legacy archive argument.
                return None;
            }
        }
    }

    // --- Priority 2: Legacy CLI flags ------------------------------------
    let action = parse_shell_args_legacy(args);
    match action {
        ShellAction::OpenArchives(paths) if paths.is_empty() => None,
        ShellAction::OpenArchives(paths) => Some(ResolvedAction::OpenArchives(paths)),
        ShellAction::Open(paths) if paths.is_empty() => None,
        ShellAction::Extract(paths) if paths.is_empty() => None,
        ShellAction::ExtractHere(paths) if paths.is_empty() => None,
        ShellAction::CompressZip(paths) if paths.is_empty() => None,
        ShellAction::Compress(paths) if paths.is_empty() => None,
        other => Some(into_resolved(other)),
    }
}

/// Extract the value following a named flag (e.g. `--shell-action-file <path>`).
/// Returns `None` if the flag is not present or no value follows.
fn extract_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).map(|s| s.as_str())
}

/// Parse action file bytes into a ShellActionPayload.
/// Platform-independent core; used by both the Windows and non-Windows paths
/// in `read_action_file_action`, and directly testable.
fn parse_action_file_bytes(data: &[u8]) -> Result<ShellActionPayload, String> {
    let (action, wide_paths) =
        shell_action_file::decode(data).map_err(|e| format!("action file decode: {e}"))?;
    let paths: Vec<String> = wide_paths
        .iter()
        .map(|w| String::from_utf16_lossy(w))
        .collect();
    Ok(ShellActionPayload {
        action: action.as_action_str().to_string(),
        format: None,
        paths,
    })
}

/// Read a `--shell-action-file` and convert to a `ShellActionPayload`.
/// On error, logs diagnostics and returns the error.
fn read_action_file_action(file_path: &str) -> Result<ShellActionPayload, String> {
    let path = std::path::Path::new(file_path);

    #[cfg(target_os = "windows")]
    {
        let (action, paths) =
            shell_action_file::read_action_file(path).map_err(|e| format!("{e}"))?;
        let path_strings: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        return Ok(ShellActionPayload {
            action: action.as_action_str().to_string(),
            format: None,
            paths: path_strings,
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On non-Windows, action files can still be read for testing.
        let data = std::fs::read(path).map_err(|e| format!("cannot read {file_path}: {e}"))?;
        let payload = parse_action_file_bytes(&data)?;
        // Best-effort delete even on non-Windows (for test cleanup).
        let _ = std::fs::remove_file(path);
        Ok(payload)
    }
}

/// Legacy parser for `/flag <paths...>` and bare `<paths...>` arguments.
fn parse_shell_args_legacy(args: &[String]) -> ShellAction {
    match args.first().map(|s| s.as_str()) {
        Some("/open") => {
            let paths: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|a| {
                    let p = std::path::Path::new(a);
                    p.exists() && !p.is_dir()
                })
                .cloned()
                .collect();
            ShellAction::Open(paths)
        }
        Some("/extract") => {
            let paths: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|a| {
                    let p = std::path::Path::new(a);
                    p.exists() && !p.is_dir()
                })
                .cloned()
                .collect();
            ShellAction::Extract(paths)
        }
        Some("/extract-here") => {
            let paths: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|a| {
                    let p = std::path::Path::new(a);
                    p.exists() && !p.is_dir()
                })
                .cloned()
                .collect();
            ShellAction::ExtractHere(paths)
        }
        Some("/compress-zip") => {
            let paths: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|a| std::path::Path::new(a).exists())
                .cloned()
                .collect();
            ShellAction::CompressZip(paths)
        }
        Some("/compress") => {
            let paths: Vec<String> = args
                .iter()
                .skip(1)
                .filter(|a| std::path::Path::new(a).exists())
                .cloned()
                .collect();
            ShellAction::Compress(paths)
        }
        _ => {
            let paths: Vec<String> = args
                .iter()
                .filter(|a| {
                    let p = std::path::Path::new(a);
                    p.exists() && !p.is_dir()
                })
                .cloned()
                .collect();
            ShellAction::OpenArchives(paths)
        }
    }
}

/// Convert a non-empty, non-OpenArchives ShellAction into a ResolvedAction.
fn into_resolved(action: ShellAction) -> ResolvedAction {
    match action {
        ShellAction::OpenArchives(paths) => ResolvedAction::OpenArchives(paths),
        ShellAction::Open(paths) => ResolvedAction::Action(ShellActionPayload {
            action: "open".into(),
            format: None,
            paths,
        }),
        ShellAction::Extract(paths) => ResolvedAction::Action(ShellActionPayload {
            action: "extract".into(),
            format: None,
            paths,
        }),
        ShellAction::ExtractHere(paths) => ResolvedAction::Action(ShellActionPayload {
            action: "extract-here".into(),
            format: None,
            paths,
        }),
        ShellAction::CompressZip(paths) => ResolvedAction::Action(ShellActionPayload {
            action: "compress-zip".into(),
            format: None,
            paths,
        }),
        ShellAction::Compress(paths) => ResolvedAction::Action(ShellActionPayload {
            action: "compress".into(),
            format: None,
            paths,
        }),
    }
}

/// Emit a `shell-action` event to the frontend.
/// For `OpenArchives` we emit `opened-archives`; for explicit actions we
/// emit `shell-action`.
fn emit_resolved_action(window: &tauri::WebviewWindow, resolved: &ResolvedAction) {
    match resolved {
        ResolvedAction::OpenArchives(paths) => {
            if !paths.is_empty() {
                let _ = window.emit("opened-archives", paths);
            }
        }
        ResolvedAction::Action(payload) => {
            let _ = window.emit("shell-action", payload);
        }
    }
}

/// Run the Tauri application.
pub fn run() {
    // --- Collect and parse cold-start command-line args ---
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let resolved = resolve_shell_action(&raw_args);

    // Legacy cold-start file paths (for `get_opened_archives` backward compat).
    let cold_args: Vec<String> = if let Some(ResolvedAction::OpenArchives(ref paths)) = resolved {
        paths.clone()
    } else {
        raw_args
            .iter()
            .filter(|a| {
                let p = std::path::Path::new(a);
                p.exists() && !p.is_dir()
            })
            .cloned()
            .collect()
    };

    // Cold-start shell action (only when an explicit action was found).
    let cold_shell: Option<ShellActionPayload> = match &resolved {
        Some(ResolvedAction::Action(payload)) => Some(payload.clone()),
        _ => None,
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
        // Save/restore window position, size, and maximized state
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        // Single-instance: second instance passes file paths + shell action to existing window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let args: Vec<String> = argv.into_iter().skip(1).collect();
            if let Some(resolved) = resolve_shell_action(&args) {
                if let Some(window) = app.get_webview_window("main") {
                    emit_resolved_action(&window, &resolved);
                }
            }
        }))
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::formats::get_formats,
            commands::associations::get_file_associations,
            commands::associations::set_file_association,
            commands::associations::open_association_settings,
            commands::list::list_archive,
            commands::list::list_archive_stream,
            commands::test::test_archive,
            commands::extract::extract_archive,
            commands::cancel::cancel_task,
            commands::compress::compress_archive,
            commands::app::get_opened_archives,
            commands::app::get_shell_action,
            commands::app::get_version,
            commands::extract_entries::extract_entries,
            commands::preview_entry::preview_entry,
            commands::drag::prepare_drag_entries,
            commands::drag::cleanup_drag_temp_dir,
            commands::drag::cleanup_stale_drag_temp_dirs,
            commands::shell_menu::get_shell_menu_state,
            commands::shell_menu::set_shell_menu,
        ])
        .setup(move |app| {
            // Store cold-start file paths (if any) into state.
            if !cold_args.is_empty() {
                if let Some(state) = app.try_state::<state::AppState>() {
                    if let Ok(mut pending) = state.pending_archives.lock() {
                        pending.extend(cold_args.clone());
                    }
                }
            }
            // Store cold-start shell action (if any) into state.
            if let Some(ref action) = cold_shell {
                if let Some(state) = app.try_state::<state::AppState>() {
                    if let Ok(mut pending) = state.pending_shell_action.lock() {
                        *pending = Some(action.clone());
                    }
                }
            }
            // Clean up stale drag-out temp directories on startup
            tauri::async_runtime::spawn(async {
                let _ = commands::drag::cleanup_stale_drag_temp_dirs().await;
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building GeeZipX GUI");

    // Run the app with a callback so we can handle macOS Opened events.
    app.run(|app_handle, event| {
        // Suppress unused-variable warnings on non-macOS platforms
        // (app_handle and event are only consumed by the macOS Opened handler).
        let _ = (&app_handle, &event);
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = event {
            let paths: Vec<String> = urls
                .iter()
                .filter_map(|u| {
                    let path = u.to_file_path().ok()?;
                    if path.exists() && !path.is_dir() {
                        Some(path.to_string_lossy().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if !paths.is_empty() {
                if let Some(state) = app_handle.try_state::<state::AppState>() {
                    if let Ok(mut pending) = state.pending_archives.lock() {
                        pending.extend(paths.clone());
                    }
                }
                let _ = app_handle.emit("opened-archives", &paths);
            }
        }
    });
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use shell_action_file::{encode, ShellActionFileAction};
    use std::path::Path;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create a real file in `dir` with dummy content (so `Path::exists()` passes).
    fn touch(dir: &Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"dummy").unwrap();
        p
    }

    /// Build a binary action file on disk and return its path.
    fn write_action_file(
        dir: &Path,
        name: &str,
        action: ShellActionFileAction,
        paths: &[&str],
    ) -> std::path::PathBuf {
        let wide: Vec<Vec<u16>> = paths.iter().map(|s| s.encode_utf16().collect()).collect();
        let data = encode(action, &wide).unwrap();
        let file_path = dir.join(name);
        std::fs::write(&file_path, &data).unwrap();
        file_path
    }

    fn args_of(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    // -----------------------------------------------------------------------
    // parse_action_file_bytes
    // -----------------------------------------------------------------------

    #[test]
    fn parse_bytes_compress_action() {
        let paths: Vec<Vec<u16>> = vec!["/tmp/a.txt".encode_utf16().collect()];
        let data = encode(ShellActionFileAction::Compress, &paths).unwrap();
        let payload = parse_action_file_bytes(&data).unwrap();
        assert_eq!(payload.action, "compress");
        assert_eq!(payload.paths.len(), 1);
        assert!(payload.paths[0].contains("a.txt"));
    }

    /// Lock the contract: a CompressZip action file must produce
    /// action="compress-zip" (not "compress", not None).
    #[test]
    fn parse_bytes_compress_zip_action() {
        let paths: Vec<Vec<u16>> = vec!["/tmp/data.7z".encode_utf16().collect()];
        let data = encode(ShellActionFileAction::CompressZip, &paths).unwrap();
        let payload = parse_action_file_bytes(&data).unwrap();
        assert_eq!(payload.action, "compress-zip");
        assert_eq!(payload.paths.len(), 1);
        assert!(payload.paths[0].contains("data.7z"));
        assert_eq!(payload.format, None);
    }

    #[test]
    fn parse_bytes_bad_magic_returns_err() {
        let data = vec![0u8; 20];
        let err = parse_action_file_bytes(&data).unwrap_err();
        assert!(err.contains("bad magic") || err.contains("decode"), "{err}");
    }

    #[test]
    fn parse_bytes_empty_returns_err() {
        let err = parse_action_file_bytes(&[]).unwrap_err();
        assert!(!err.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_shell_action — legacy flags
    // -----------------------------------------------------------------------

    #[test]
    fn legacy_compress_with_real_paths() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = touch(dir.path(), "a.txt");
        let f2 = touch(dir.path(), "b.txt");
        let args = args_of(&["/compress", &f1.to_string_lossy(), &f2.to_string_lossy()]);
        let resolved = resolve_shell_action(&args).unwrap();
        match resolved {
            ResolvedAction::Action(p) => {
                assert_eq!(p.action, "compress");
                assert_eq!(p.paths.len(), 2);
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    #[test]
    fn legacy_compress_zip_with_real_paths() {
        let dir = tempfile::tempdir().unwrap();
        let f = touch(dir.path(), "data.bin");
        let args = args_of(&["/compress-zip", &f.to_string_lossy()]);
        let resolved = resolve_shell_action(&args).unwrap();
        match resolved {
            ResolvedAction::Action(p) => {
                assert_eq!(p.action, "compress-zip");
                assert_eq!(p.paths.len(), 1);
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    #[test]
    fn legacy_extract_with_real_paths() {
        let dir = tempfile::tempdir().unwrap();
        let f = touch(dir.path(), "archive.zip");
        let args = args_of(&["/extract", &f.to_string_lossy()]);
        let resolved = resolve_shell_action(&args).unwrap();
        match resolved {
            ResolvedAction::Action(p) => {
                assert_eq!(p.action, "extract");
                assert_eq!(p.paths.len(), 1);
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // resolve_shell_action — bare paths → OpenArchives
    // -----------------------------------------------------------------------

    #[test]
    fn bare_paths_resolve_to_open_archives() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = touch(dir.path(), "archive.zip");
        let f2 = touch(dir.path(), "data.7z");
        let args = args_of(&[&f1.to_string_lossy(), &f2.to_string_lossy()]);
        let resolved = resolve_shell_action(&args).unwrap();
        match resolved {
            ResolvedAction::OpenArchives(paths) => {
                assert_eq!(paths.len(), 2);
            }
            other => panic!("expected OpenArchives, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // resolve_shell_action — empty / non-existent paths
    // -----------------------------------------------------------------------

    #[test]
    fn empty_args_returns_none() {
        let args: Vec<String> = vec![];
        assert!(resolve_shell_action(&args).is_none());
    }

    #[test]
    fn legacy_flag_no_existing_paths_returns_none() {
        let args = args_of(&["/compress", "/nonexistent/path/12345/foo.txt"]);
        assert!(resolve_shell_action(&args).is_none());
    }

    #[test]
    fn bare_non_existing_paths_returns_none() {
        let args = args_of(&["/nonexistent/path/xyz.7z"]);
        // Bare paths are filtered by Path::exists() → empty → None.
        assert!(resolve_shell_action(&args).is_none());
    }

    // -----------------------------------------------------------------------
    // resolve_shell_action — --shell-action-file
    // -----------------------------------------------------------------------

    #[test]
    fn shell_action_file_has_priority_over_legacy() {
        let dir = tempfile::tempdir().unwrap();
        let action_path = write_action_file(
            dir.path(),
            "priority_test.gzsa",
            ShellActionFileAction::Compress,
            &["/tmp/from_action.txt"],
        );
        // Also pass legacy /compress-zip — action file must win.
        let legacy_file = touch(dir.path(), "legacy.dat");
        let args = args_of(&[
            "--shell-action-file",
            &action_path.to_string_lossy(),
            "/compress-zip",
            &legacy_file.to_string_lossy(),
        ]);
        let resolved = resolve_shell_action(&args).unwrap();
        match resolved {
            ResolvedAction::Action(p) => {
                assert_eq!(p.action, "compress"); // from action file, NOT compress-zip
                assert_eq!(p.paths.len(), 1);
                assert!(p.paths[0].contains("from_action.txt"));
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    #[test]
    fn shell_action_file_missing_returns_none() {
        let args = args_of(&["--shell-action-file", "/nonexistent/dir/missing.gzsa"]);
        // Should return None (no fallthrough).
        assert!(resolve_shell_action(&args).is_none());
    }

    #[test]
    fn shell_action_file_corrupt_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("corrupt.gzsa");
        std::fs::write(&bad, b"this is not a valid action file").unwrap();
        let args = args_of(&["--shell-action-file", &bad.to_string_lossy()]);
        assert!(resolve_shell_action(&args).is_none());
    }

    #[test]
    fn shell_action_file_empty_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.gzsa");
        std::fs::write(&empty, b"").unwrap();
        let args = args_of(&["--shell-action-file", &empty.to_string_lossy()]);
        assert!(resolve_shell_action(&args).is_none());
    }

    #[test]
    fn shell_action_file_missing_value_returns_none() {
        // Flag present but no path following it.
        let args = args_of(&["--shell-action-file"]);
        // extract_flag_value returns None → falls through to legacy.
        // With no other args, this returns None.
        assert!(resolve_shell_action(&args).is_none());
    }

    #[test]
    fn duplicate_shell_action_file_takes_first() {
        let dir = tempfile::tempdir().unwrap();
        let first = write_action_file(
            dir.path(),
            "first.gzsa",
            ShellActionFileAction::Compress,
            &["/tmp/first.txt"],
        );
        let _second = write_action_file(
            dir.path(),
            "second.gzsa",
            ShellActionFileAction::CompressZip,
            &["/tmp/second.txt"],
        );
        let args = args_of(&[
            "--shell-action-file",
            &first.to_string_lossy(),
            "--shell-action-file",
            &_second.to_string_lossy(),
        ]);
        let resolved = resolve_shell_action(&args).unwrap();
        match resolved {
            ResolvedAction::Action(p) => {
                assert_eq!(p.action, "compress"); // first wins
                assert!(p.paths[0].contains("first.txt"));
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // cold / warm path — same resolve_shell_action function
    // -----------------------------------------------------------------------

    /// The single-instance callback in `run()` calls `resolve_shell_action`,
    /// which is the same function used at cold start.  This test proves
    /// the function works correctly for both paths by calling it directly.
    #[test]
    fn same_function_for_cold_and_warm() {
        let dir = tempfile::tempdir().unwrap();

        // Cold-start simulation: bare archive paths.
        let f1 = touch(dir.path(), "cold.zip");
        let cold_args = args_of(&[&f1.to_string_lossy()]);
        let cold = resolve_shell_action(&cold_args).unwrap();
        assert!(matches!(cold, ResolvedAction::OpenArchives(_)));

        // Warm (single-instance callback) simulation: --shell-action-file.
        let action_path = write_action_file(
            dir.path(),
            "warm_test.gzsa",
            ShellActionFileAction::CompressZip,
            &["/tmp/warm.txt"],
        );
        let warm_args = args_of(&["--shell-action-file", &action_path.to_string_lossy()]);
        let warm = resolve_shell_action(&warm_args).unwrap();
        match warm {
            ResolvedAction::Action(p) => {
                assert_eq!(p.action, "compress-zip");
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // extract_flag_value edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn extract_flag_value_absent() {
        let args = args_of(&["/compress", "file.txt"]);
        assert!(extract_flag_value(&args, "--shell-action-file").is_none());
    }

    #[test]
    fn extract_flag_value_trailing_flag() {
        let args = args_of(&["/compress", "--shell-action-file"]);
        // Flag is present but nothing follows → None.
        assert!(extract_flag_value(&args, "--shell-action-file").is_none());
    }
}
