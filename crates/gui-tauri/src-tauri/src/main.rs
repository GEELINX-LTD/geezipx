#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Phase B: COM LocalServer interception.
    // When launched by Explorer via DelegateExecute, COM appends "-Embedding"
    // (or "/Embedding") to the command line.  Detect it and enter the COM
    // server instead of the normal Tauri GUI.
    let raw_args: Vec<String> = std::env::args().collect();
    if geezipx_gui_lib::is_embedding_arg(&raw_args[1..]) {
        #[cfg(target_os = "windows")]
        {
            geezipx_gui_lib::run_com_server();
            // run_com_server() does not return on Windows.
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Non-Windows: Embedding flag is unexpected; fall through to
            // normal GUI launch (should never happen in practice).
            eprintln!("warning: -Embedding flag received on non-Windows platform, launching GUI");
        }
    }

    geezipx_gui_lib::run();
}
