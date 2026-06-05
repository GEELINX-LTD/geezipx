//! GeeZipX GUI — Tauri v2 application backend.
//!
//! This crate is a thin bridge between the Tauri frontend and `geezipx-core`.
//! No compression/decompression logic lives here.

pub mod commands;
pub mod state;

/// Run the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![commands::formats::get_formats,])
        .run(tauri::generate_context!())
        .expect("error while running GeeZipX GUI");
}
