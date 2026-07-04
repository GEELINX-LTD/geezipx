//! GeeZipX GUI — Tauri v2 application backend.
//!
//! This crate is a thin bridge between the Tauri frontend and `geezipx-core`.
//! No compression/decompression logic lives here.

use tauri::Emitter;
use tauri::Manager;

pub mod commands;
pub mod state;

/// Run the Tauri application.
pub fn run() {
    // --- Collect cold-start file args (Windows/Linux) ---
    let cold_args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| {
            let p = std::path::Path::new(a);
            p.exists() && !p.is_dir()
        })
        .collect();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
        // Save/restore window position, size, and maximized state
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        // Single-instance: second instance passes file paths to existing window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let paths: Vec<String> = argv
                .into_iter()
                .skip(1)
                .filter(|a| {
                    let p = std::path::Path::new(a);
                    p.exists() && !p.is_dir()
                })
                .collect();
            if !paths.is_empty() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("opened-archives", &paths);
                }
            }
        }))
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::formats::get_formats,
            commands::list::list_archive,
            commands::test::test_archive,
            commands::extract::extract_archive,
            commands::cancel::cancel_task,
            commands::compress::compress_archive,
            commands::app::get_opened_archives,
            commands::extract_entries::extract_entries,
            commands::preview_entry::preview_entry,
            commands::drag::prepare_drag_entries,
            commands::drag::cleanup_drag_temp_dir,
            commands::drag::cleanup_stale_drag_temp_dirs,
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
