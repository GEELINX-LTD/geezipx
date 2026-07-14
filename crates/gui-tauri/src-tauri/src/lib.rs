//! GeeZipX GUI — Tauri v2 application backend.
//!
//! This crate is a thin bridge between the Tauri frontend and `geezipx-core`.
//! No compression/decompression logic lives here.

use serde::Serialize;
use tauri::Emitter;
use tauri::Manager;

pub mod commands;
pub mod state;

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse command-line arguments to determine the shell action.
///
/// Recognised patterns:
/// - `/open <paths...>`         — browse archive
/// - `/extract <paths...>`      — jump to extract page
/// - `/extract-here <paths...>` — smart extract to current folder
/// - `/compress-zip <paths...>` — headless quick ZIP
/// - `/compress <paths...>`     — jump to compress page (files + dirs)
/// - `<paths...>`               — legacy: open archives for browsing
fn parse_shell_args(args: &[String]) -> ShellAction {
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

/// Emit a `shell-action` event to the frontend.
/// For `OpenArchives` (legacy, no flag) we only emit `opened-archives`.
fn emit_shell_action(window: &tauri::WebviewWindow, action: &ShellAction) {
    // Legacy mode — no explicit flag, treat as plain file-open.
    if let ShellAction::OpenArchives(paths) = action {
        if !paths.is_empty() {
            let _ = window.emit("opened-archives", &paths);
        }
        return;
    }

    let payload = match action {
        ShellAction::Open(paths) => ShellActionPayload {
            action: "open".into(),
            format: None,
            paths: paths.clone(),
        },
        ShellAction::Extract(paths) => ShellActionPayload {
            action: "extract".into(),
            format: None,
            paths: paths.clone(),
        },
        ShellAction::ExtractHere(paths) => ShellActionPayload {
            action: "extract-here".into(),
            format: None,
            paths: paths.clone(),
        },
        ShellAction::CompressZip(paths) => ShellActionPayload {
            action: "compress-zip".into(),
            format: None,
            paths: paths.clone(),
        },
        ShellAction::Compress(paths) => ShellActionPayload {
            action: "compress".into(),
            format: None,
            paths: paths.clone(),
        },
        ShellAction::OpenArchives(_) => return,
    };
    if !payload.paths.is_empty() {
        let _ = window.emit("shell-action", &payload);
    }
}

/// Run the Tauri application.
pub fn run() {
    // --- Collect and parse cold-start command-line args ---
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let shell_action = parse_shell_args(&raw_args);

    // Legacy cold-start file paths (for `get_opened_archives` backward compat).
    let cold_args: Vec<String> = raw_args
        .iter()
        .filter(|a| {
            let p = std::path::Path::new(a);
            p.exists() && !p.is_dir()
        })
        .cloned()
        .collect();

    // Cold-start shell action (only when an explicit flag was used).
    let cold_shell: Option<ShellActionPayload> = match &shell_action {
        ShellAction::OpenArchives(_) => None,
        ShellAction::Open(paths) if !paths.is_empty() => Some(ShellActionPayload {
            action: "open".into(),
            format: None,
            paths: paths.clone(),
        }),
        ShellAction::Extract(paths) if !paths.is_empty() => Some(ShellActionPayload {
            action: "extract".into(),
            format: None,
            paths: paths.clone(),
        }),
        ShellAction::ExtractHere(paths) if !paths.is_empty() => Some(ShellActionPayload {
            action: "extract-here".into(),
            format: None,
            paths: paths.clone(),
        }),
        ShellAction::CompressZip(paths) if !paths.is_empty() => Some(ShellActionPayload {
            action: "compress-zip".into(),
            format: None,
            paths: paths.clone(),
        }),
        ShellAction::Compress(paths) if !paths.is_empty() => Some(ShellActionPayload {
            action: "compress".into(),
            format: None,
            paths: paths.clone(),
        }),
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
            let action = parse_shell_args(&args);
            if let Some(window) = app.get_webview_window("main") {
                emit_shell_action(&window, &action);
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
