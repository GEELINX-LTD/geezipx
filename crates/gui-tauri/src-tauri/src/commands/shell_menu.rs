//! Windows Shell context menu (right-click) management commands.
//!
//! Lets the user enable / disable individual Explorer right-click verbs
//! from the Settings page.  The runtime writes directly to
//! `HKCU\Software\Classes` via the [`windows-registry`] safe API so no
//! administrator privileges are required and no external `reg.exe` process
//! is spawned.
//!
//! Verbs (registry key suffix is always PascalCase)
//! ------------------------------------------------
//! | Verb            | Key suffix     | Target                  | CLI flag         |
//! |-----------------|----------------|-------------------------|------------------|
//! | `extract`       | Extract        | archive file extensions | `/extract`       |
//! | `extract_here`  | ExtractHere    | archive file extensions | `/extract-here`  |
//! | `compress_zip`  | CompressZip    | `*` and `Directory`     | `/compress-zip`  |
//! | `compress`      | Compress       | `*` and `Directory`     | `/compress`      |
//!
//! On Windows 11 these entries appear under "Show more options" (the classic
//! context menu) — this is a system limitation that cannot be worked around
//! without an IExplorerCommand COM server / MSIX package.
//!
//! Non-Windows platforms return `supported: false` — the commands exist so
//! the frontend can call them unconditionally without platform checks.
//!
//! Sentinel
//! --------
//! After every successful `set_shell_menu` call a sentinel value is written
//! to `HKCU\Software\Classes\GeeZipX\ShellMenu\Configured=1`.  This prevents
//! the NSIS installer from re-registering default verbs on upgrade when the
//! user has deliberately turned everything off.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Public types (serialised to the frontend)
// ---------------------------------------------------------------------------

/// Individual shell menu verb that can be toggled on/off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellMenuVerb {
    /// "Extract to..." — jump to extract page with archive-path pre-filled.
    Extract,
    /// "Extract here" — smart extract to the archive's parent folder.
    ExtractHere,
    /// "Compress as ZIP" — headless quick ZIP with default settings.
    CompressZip,
    /// "Compress as..." — jump to compress page with paths pre-filled.
    Compress,
}

/// Returned by `get_shell_menu_state`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellMenuState {
    /// `"windows"` | `"linux"` | `"macos"` | `"unknown"`.
    pub platform: String,
    /// `true` on Windows; `false` everywhere else.
    pub supported: bool,
    /// Which verbs are currently registered (only meaningful on Windows).
    pub registered: Vec<ShellMenuVerb>,
    /// Archive extensions that extract verbs apply to.
    pub archive_extensions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Pure helpers (testable, no platform-specific code)
// ---------------------------------------------------------------------------

/// Archive extensions for which extract menus are registered.
const ARCHIVE_EXTS: &[&str] = &[
    ".zip", ".zipx", ".tar", ".gz", ".bz2", ".br", ".lz4", ".zst", ".xz", ".lzma", ".lz", ".7z",
    ".rar", ".cab", ".asar", ".deb", ".cpio", ".iso", ".udf", ".lzh", ".lha", ".zpaq", ".wim",
    ".isz",
];

/// PascalCase registry key suffix for a verb.
///
/// These are the names used in the `GeeZipX.<suffix>` shell key (e.g.
/// `GeeZipX.ExtractHere`).  The suffix is stable and independent of serde
/// rename or `Debug` formatting — changing a `ShellMenuVerb` variant name
/// will cause a *compile error* here, not a silent registry key mismatch.
pub fn verb_key_name(verb: ShellMenuVerb) -> &'static str {
    match verb {
        ShellMenuVerb::Extract => "Extract",
        ShellMenuVerb::ExtractHere => "ExtractHere",
        ShellMenuVerb::CompressZip => "CompressZip",
        ShellMenuVerb::Compress => "Compress",
    }
}

/// Build the HKCU-relative registry key path for an extract verb on a given
/// extension.
///
/// Example: `reg_key_for_ext(".zip", ShellMenuVerb::Extract)` →
/// `Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX.Extract`
pub fn reg_key_for_ext(ext: &str, verb: ShellMenuVerb) -> String {
    let name = verb_key_name(verb);
    format!(r"Software\Classes\SystemFileAssociations\{ext}\shell\GeeZipX.{name}")
}

/// Build the HKCU-relative registry key path for a compress verb on `*`
/// (all files).
pub fn reg_key_for_any_file(verb: ShellMenuVerb) -> String {
    format!(r"Software\Classes\*\shell\GeeZipX.{}", verb_key_name(verb))
}

/// Build the HKCU-relative registry key path for a compress verb on
/// `Directory`.
pub fn reg_key_for_dir(verb: ShellMenuVerb) -> String {
    format!(
        r"Software\Classes\Directory\shell\GeeZipX.{}",
        verb_key_name(verb)
    )
}

/// HKCU-relative path of the sentinel key.  When present the NSIS installer
/// skips its default verb registration on upgrade (preserving the user's
/// choice).
pub const SENTINEL_KEY: &str = r"Software\Classes\GeeZipX\ShellMenu";
pub const SENTINEL_VALUE: &str = "Configured";
pub const SENTINEL_DATA: &str = "1";

/// Build the shell command string for a given executable path and CLI flag.
///
/// Example: `build_command(r"C:\Program Files\GeeZipX\geezipx-gui.exe", "/extract")` →
/// `"C:\Program Files\GeeZipX\geezipx-gui.exe" /extract "%1"`
pub fn build_command(exe_path: &str, cli_flag: &str) -> String {
    format!("\"{exe_path}\" {cli_flag} \"%1\"")
}

/// Shell verb name → CLI flag mapping.
pub fn cli_flag_for_verb(verb: ShellMenuVerb) -> &'static str {
    match verb {
        ShellMenuVerb::Extract => "/extract",
        ShellMenuVerb::ExtractHere => "/extract-here",
        ShellMenuVerb::CompressZip => "/compress-zip",
        ShellMenuVerb::Compress => "/compress",
    }
}

/// English display label for a verb (used as the MUIVerb registry value).
pub fn label_for_verb(verb: ShellMenuVerb) -> &'static str {
    match verb {
        ShellMenuVerb::Extract => "Extract to...",
        ShellMenuVerb::ExtractHere => "Extract here",
        ShellMenuVerb::CompressZip => "Compress as ZIP",
        ShellMenuVerb::Compress => "Compress as...",
    }
}

/// Parse a verb string (from the frontend) into a `ShellMenuVerb`.
pub fn parse_verb(s: &str) -> Option<ShellMenuVerb> {
    match s {
        "extract" => Some(ShellMenuVerb::Extract),
        "extract_here" => Some(ShellMenuVerb::ExtractHere),
        "compress_zip" => Some(ShellMenuVerb::CompressZip),
        "compress" => Some(ShellMenuVerb::Compress),
        _ => None,
    }
}

/// All four verbs in display order (ExtractHere first).
pub fn all_verbs() -> [ShellMenuVerb; 4] {
    [
        ShellMenuVerb::ExtractHere,
        ShellMenuVerb::Extract,
        ShellMenuVerb::CompressZip,
        ShellMenuVerb::Compress,
    ]
}

/// Extract-only verbs.
pub fn extract_verbs() -> [ShellMenuVerb; 2] {
    [ShellMenuVerb::ExtractHere, ShellMenuVerb::Extract]
}

/// Compress-only verbs.
pub fn compress_verbs() -> [ShellMenuVerb; 2] {
    [ShellMenuVerb::CompressZip, ShellMenuVerb::Compress]
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Return the current shell menu state (platform, supported, registered verbs).
#[tauri::command]
pub fn get_shell_menu_state() -> ShellMenuState {
    let platform = platform_name();
    let supported = platform == "windows";

    let registered = query_verbs();

    ShellMenuState {
        platform,
        supported,
        registered,
        archive_extensions: ARCHIVE_EXTS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Query registered verbs, dispatching to the Windows platform module or
/// returning an empty list on other platforms.
fn query_verbs() -> Vec<ShellMenuVerb> {
    #[cfg(target_os = "windows")]
    {
        match platform::query_registered_verbs() {
            Ok(verbs) => verbs,
            Err(e) => {
                eprintln!("warning: failed to query shell menu verbs: {e}");
                Vec::new()
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

/// Enable or disable shell context menu verbs.
///
/// - `enabled`: master on/off switch (`false` removes all verbs).
/// - `verbs`: which specific verbs to register (only meaningful when `enabled` is `true`).
///
/// On Windows the function writes / deletes registry keys under
/// `HKCU\Software\Classes`, calls `SHChangeNotify` to refresh Explorer, and
/// writes a sentinel so the NSIS installer preserves the user's choice on
/// upgrade.
///
/// On other platforms it returns `Err("unsupported platform")`.
#[tauri::command]
pub fn set_shell_menu(enabled: bool, verbs: Vec<String>) -> Result<(), String> {
    set_shell_menu_impl(enabled, verbs)
}

#[cfg(target_os = "windows")]
fn set_shell_menu_impl(enabled: bool, verbs: Vec<String>) -> Result<(), String> {
    let parsed: Vec<ShellMenuVerb> = verbs.iter().filter_map(|v| parse_verb(v)).collect();

    if enabled {
        platform::register_verbs(&parsed)?;
    } else {
        platform::remove_all_verbs()?;
    }

    // Write the sentinel so the NSIS installer knows the user has made a
    // deliberate choice (even if that choice was "turn everything off").
    platform::write_sentinel()?;

    // Notify Explorer so the menu changes take effect immediately.
    platform::notify_shell_change();

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_shell_menu_impl(_enabled: bool, _verbs: Vec<String>) -> Result<(), String> {
    Err("Shell context menu is only supported on Windows".into())
}

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

fn platform_name() -> String {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
    .into()
}

// ---------------------------------------------------------------------------
// Windows-specific implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows_registry::CURRENT_USER;

    // HRESULT value for Win32 ERROR_FILE_NOT_FOUND (0x2), as produced by
    // `HRESULT::from_win32(2)`.  Used to distinguish "key/value does not
    // exist" (a normal, non-error condition) from genuine registry failures.
    // `pub(super)` so the unit test can verify the constant value.
    pub(super) const HR_FILE_NOT_FOUND: i32 = 0x80070002u32 as i32;

    /// Returns `true` if `err` represents a missing key or value.
    fn is_not_found(err: &windows_registry::Error) -> bool {
        err.code().0 == HR_FILE_NOT_FOUND
    }

    // -- Win32 FFI for SHChangeNotify ---------------------------------------

    mod win32 {
        // SAFETY: These are well-known, stable Win32 API declarations. The
        // function signatures have been unchanged since Windows 95.
        extern "system" {
            /// Notifies the system Shell of an event. Used here to inform
            /// Explorer that file associations have changed so the context
            /// menu is refreshed immediately.
            ///
            /// <https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify>
            pub fn SHChangeNotify(
                wEventId: i32,
                uFlags: u32,
                dwItem1: *const std::ffi::c_void,
                dwItem2: *const std::ffi::c_void,
            );
        }

        /// A file type association has changed.
        pub const SHCNE_ASSOCCHANGED: i32 = 0x08000000;
        /// `dwItem1` and `dwItem2` are not used.
        pub const SHCNF_IDLIST: u32 = 0x0000;
    }

    // -- Registry helpers ---------------------------------------------------

    /// Open or create a key at `path` (HKCU-relative) and set `name` to
    /// `data`.  An empty `name` writes the default value.
    fn reg_set_string(key_path: &str, name: &str, data: &str) -> Result<(), String> {
        let key = CURRENT_USER
            .create(key_path)
            .map_err(|e| format!("failed to create key {key_path}: {e}"))?;
        key.set_string(name, data)
            .map_err(|e| format!("failed to set value '{name}' at {key_path}: {e}"))
    }

    /// Check whether a registry key exists (any content, not just a specific
    /// value).  Returns `Ok(true)` when the key is present, `Ok(false)` when
    /// it is absent, or an `Err` for access-denied / other failures.
    fn reg_key_exists(key_path: &str) -> Result<bool, String> {
        match CURRENT_USER.open(key_path) {
            Ok(_) => Ok(true),
            Err(e) if is_not_found(&e) => Ok(false),
            Err(e) => Err(format!("failed to query {key_path}: {e}")),
        }
    }

    /// Recursively delete a registry key tree.  A missing key is treated as
    /// success; any other failure is propagated.
    fn reg_delete_tree(key_path: &str) -> Result<(), String> {
        match CURRENT_USER.remove_tree(key_path) {
            Ok(()) => Ok(()),
            Err(e) if is_not_found(&e) => Ok(()),
            Err(e) => Err(format!("failed to delete {key_path}: {e}")),
        }
    }

    // -- Verb registration --------------------------------------------------

    /// Resolve the current executable path for command registration.
    fn our_exe() -> String {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "geezipx-gui.exe".into())
    }

    /// Write a single verb for one extract extension.
    fn write_extract_verb(ext: &str, verb: ShellMenuVerb, exe: &str) -> Result<(), String> {
        let root = reg_key_for_ext(ext, verb);
        let label = label_for_verb(verb);
        let flag = cli_flag_for_verb(verb);
        let cmd = build_command(exe, flag);

        let key = CURRENT_USER
            .create(&root)
            .map_err(|e| format!("failed to create key {root}: {e}"))?;
        key.set_string("", label)
            .map_err(|e| format!("failed to set default value at {root}: {e}"))?;
        key.set_string("MUIVerb", label)
            .map_err(|e| format!("failed to set MUIVerb at {root}: {e}"))?;
        key.set_string("Icon", &format!("\"{exe}\",0"))
            .map_err(|e| format!("failed to set Icon at {root}: {e}"))?;

        let cmd_key_path = format!("{root}\\command");
        let cmd_key = CURRENT_USER
            .create(&cmd_key_path)
            .map_err(|e| format!("failed to create key {cmd_key_path}: {e}"))?;
        cmd_key
            .set_string("", &cmd)
            .map_err(|e| format!("failed to set default value at {cmd_key_path}: {e}"))?;

        Ok(())
    }

    /// Write a single compress verb for `*` and `Directory`.
    fn write_compress_verb(verb: ShellMenuVerb, exe: &str) -> Result<(), String> {
        let label = label_for_verb(verb);
        let flag = cli_flag_for_verb(verb);
        let cmd = build_command(exe, flag);

        for root_path in [reg_key_for_any_file(verb), reg_key_for_dir(verb)] {
            let key = CURRENT_USER
                .create(&root_path)
                .map_err(|e| format!("failed to create key {root_path}: {e}"))?;
            key.set_string("", label)
                .map_err(|e| format!("failed to set default value at {root_path}: {e}"))?;
            key.set_string("MUIVerb", label)
                .map_err(|e| format!("failed to set MUIVerb at {root_path}: {e}"))?;
            key.set_string("Icon", &format!("\"{exe}\",0"))
                .map_err(|e| format!("failed to set Icon at {root_path}: {e}"))?;

            // MultiSelectModel is an optional enhancement for the Compress
            // verb — a failure to write it is logged but does not abort the
            // registration.
            if matches!(verb, ShellMenuVerb::Compress) {
                if let Err(e) = key.set_string("MultiSelectModel", "Player") {
                    eprintln!("warning: failed to set MultiSelectModel at {root_path}: {e}");
                }
            }

            let cmd_key_path = format!("{root_path}\\command");
            let cmd_key = CURRENT_USER
                .create(&cmd_key_path)
                .map_err(|e| format!("failed to create key {cmd_key_path}: {e}"))?;
            cmd_key
                .set_string("", &cmd)
                .map_err(|e| format!("failed to set default value at {cmd_key_path}: {e}"))?;
        }

        Ok(())
    }

    // -- Sentinel -----------------------------------------------------------

    /// Write the sentinel value so the NSIS installer knows the user has
    /// deliberately configured the shell menu (even if they turned it off).
    pub fn write_sentinel() -> Result<(), String> {
        let key = CURRENT_USER
            .create(SENTINEL_KEY)
            .map_err(|e| format!("failed to create sentinel key: {e}"))?;
        key.set_string(SENTINEL_VALUE, SENTINEL_DATA)
            .map_err(|e| format!("failed to set sentinel value: {e}"))
    }

    /// Check whether the sentinel key exists.
    pub fn sentinel_exists() -> bool {
        match CURRENT_USER.open(SENTINEL_KEY) {
            Ok(key) => key.get_string(SENTINEL_VALUE).is_ok(),
            Err(_) => false,
        }
    }

    // -- Public platform API ------------------------------------------------

    /// Check which verbs are currently registered.
    ///
    /// Returns `Ok(verbs)` on success or `Err` when a query fails with an
    /// error other than "not found" (e.g. access denied).  Callers that need
    /// to preserve the [`ShellMenuState`] JSON shape can fall back to an
    /// empty list on error.
    pub fn query_registered_verbs() -> Result<Vec<ShellMenuVerb>, String> {
        let mut registered = Vec::new();

        for verb in all_verbs() {
            let exists = match verb {
                ShellMenuVerb::Extract | ShellMenuVerb::ExtractHere => {
                    let mut any = false;
                    for ext in ARCHIVE_EXTS {
                        match reg_key_exists(&reg_key_for_ext(ext, verb)) {
                            Ok(true) => {
                                any = true;
                                break;
                            }
                            Ok(false) => continue,
                            Err(e) => return Err(e),
                        }
                    }
                    any
                }
                ShellMenuVerb::CompressZip | ShellMenuVerb::Compress => {
                    let file_path = reg_key_for_any_file(verb);
                    let dir_path = reg_key_for_dir(verb);
                    let file_ok = reg_key_exists(&file_path)?;
                    let dir_ok = reg_key_exists(&dir_path)?;
                    file_ok || dir_ok
                }
            };
            if exists {
                registered.push(verb);
            }
        }

        Ok(registered)
    }

    /// Register the given set of verbs. Removes all existing GeeZipX verbs
    /// first so stale keys from a previous configuration are cleaned up.
    ///
    /// The remove-then-write sequence is not wrapped in a registry transaction
    /// because `windows-registry` does not expose transaction support for
    /// [`Key::remove_tree`] — the underlying `RegDeleteTreeW` does not accept
    /// a transaction handle.  A partial failure during registration leaves the
    /// shell menu in an intermediate state (some keys removed, not all
    /// re-created), which the user can repair by toggling the setting again.
    pub fn register_verbs(verbs: &[ShellMenuVerb]) -> Result<(), String> {
        let exe = our_exe();

        // Start from a clean slate.
        remove_all_verbs()?;

        for &verb in verbs {
            match verb {
                ShellMenuVerb::Extract | ShellMenuVerb::ExtractHere => {
                    for ext in ARCHIVE_EXTS {
                        write_extract_verb(ext, verb, &exe)?;
                    }
                }
                ShellMenuVerb::CompressZip | ShellMenuVerb::Compress => {
                    write_compress_verb(verb, &exe)?;
                }
            }
        }

        Ok(())
    }

    /// Remove ALL GeeZipX shell verbs from HKCU.  Missing keys are silently
    /// skipped; any real failure (e.g. access denied) is propagated.
    pub fn remove_all_verbs() -> Result<(), String> {
        for ext in ARCHIVE_EXTS {
            for verb in extract_verbs() {
                reg_delete_tree(&reg_key_for_ext(ext, verb))?;
            }
        }
        for verb in compress_verbs() {
            reg_delete_tree(&reg_key_for_any_file(verb))?;
            reg_delete_tree(&reg_key_for_dir(verb))?;
        }
        Ok(())
    }

    /// Tell Explorer to refresh its icon / association cache.
    pub fn notify_shell_change() {
        // SAFETY: SHChangeNotify is a well-known Win32 API. We pass null
        // pointers for dwItem1/dwItem2 because SHCNE_ASSOCCHANGED with
        // SHCNF_IDLIST does not require them.
        unsafe {
            win32::SHChangeNotify(
                win32::SHCNE_ASSOCCHANGED,
                win32::SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- verb_key_name ------------------------------------------------------

    #[test]
    fn test_verb_key_name_pascal_case() {
        assert_eq!(verb_key_name(ShellMenuVerb::Extract), "Extract");
        assert_eq!(verb_key_name(ShellMenuVerb::ExtractHere), "ExtractHere");
        assert_eq!(verb_key_name(ShellMenuVerb::CompressZip), "CompressZip");
        assert_eq!(verb_key_name(ShellMenuVerb::Compress), "Compress");
    }

    // -- reg key paths (now HKCU-relative) ----------------------------------

    #[test]
    fn test_reg_key_for_ext() {
        let key = reg_key_for_ext(".zip", ShellMenuVerb::Extract);
        assert_eq!(
            key,
            r"Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX.Extract"
        );
    }

    #[test]
    fn test_reg_key_for_any_file() {
        assert_eq!(
            reg_key_for_any_file(ShellMenuVerb::CompressZip),
            r"Software\Classes\*\shell\GeeZipX.CompressZip"
        );
    }

    #[test]
    fn test_reg_key_for_dir() {
        assert_eq!(
            reg_key_for_dir(ShellMenuVerb::Compress),
            r"Software\Classes\Directory\shell\GeeZipX.Compress"
        );
    }

    // -- registry path format invariants ------------------------------------

    #[test]
    fn test_reg_paths_are_hkcu_relative() {
        // All path functions must return HKCU-relative paths (no HKCU prefix).
        for ext in ARCHIVE_EXTS {
            for verb in extract_verbs() {
                let p = reg_key_for_ext(ext, verb);
                assert!(
                    !p.to_lowercase().starts_with("hkcu"),
                    "path {p:?} must not contain HKCU prefix"
                );
                assert!(
                    p.starts_with("Software\\Classes\\"),
                    "path {p:?} must start with Software\\Classes\\"
                );
            }
        }
        for verb in compress_verbs() {
            let p = reg_key_for_any_file(verb);
            assert!(
                !p.to_lowercase().starts_with("hkcu"),
                "path {p:?} must not contain HKCU prefix"
            );
            let p = reg_key_for_dir(verb);
            assert!(
                !p.to_lowercase().starts_with("hkcu"),
                "path {p:?} must not contain HKCU prefix"
            );
        }
        assert!(
            !SENTINEL_KEY.to_lowercase().starts_with("hkcu"),
            "sentinel key must not contain HKCU prefix"
        );
    }

    // -- build_command ------------------------------------------------------

    #[test]
    fn test_build_command() {
        let cmd = build_command(r"C:\Program Files\GeeZipX\geezipx-gui.exe", "/extract");
        assert_eq!(
            cmd,
            r#""C:\Program Files\GeeZipX\geezipx-gui.exe" /extract "%1""#
        );
    }

    #[test]
    fn test_build_command_quoting() {
        let cmd = build_command(r"C:\My Programs\GeeZipX\geezipx-gui.exe", "/compress");
        assert!(cmd.starts_with('"'));
        assert!(cmd.contains("\" /compress \"%1\""));
    }

    // -- cli_flag_for_verb --------------------------------------------------

    #[test]
    fn test_cli_flag_for_verb() {
        assert_eq!(cli_flag_for_verb(ShellMenuVerb::Extract), "/extract");
        assert_eq!(
            cli_flag_for_verb(ShellMenuVerb::ExtractHere),
            "/extract-here"
        );
        assert_eq!(
            cli_flag_for_verb(ShellMenuVerb::CompressZip),
            "/compress-zip"
        );
        assert_eq!(cli_flag_for_verb(ShellMenuVerb::Compress), "/compress");
    }

    // -- label_for_verb -----------------------------------------------------

    #[test]
    fn test_label_for_verb_english() {
        assert_eq!(label_for_verb(ShellMenuVerb::Extract), "Extract to...");
        assert_eq!(label_for_verb(ShellMenuVerb::ExtractHere), "Extract here");
        assert_eq!(
            label_for_verb(ShellMenuVerb::CompressZip),
            "Compress as ZIP"
        );
        assert_eq!(label_for_verb(ShellMenuVerb::Compress), "Compress as...");
    }

    // -- parse_verb ---------------------------------------------------------

    #[test]
    fn test_parse_verb_valid() {
        assert_eq!(parse_verb("extract"), Some(ShellMenuVerb::Extract));
        assert_eq!(parse_verb("extract_here"), Some(ShellMenuVerb::ExtractHere));
        assert_eq!(parse_verb("compress_zip"), Some(ShellMenuVerb::CompressZip));
        assert_eq!(parse_verb("compress"), Some(ShellMenuVerb::Compress));
    }

    #[test]
    fn test_parse_verb_invalid() {
        assert_eq!(parse_verb(""), None);
        assert_eq!(parse_verb("open"), None);
        assert_eq!(parse_verb("Extract"), None); // case-sensitive
    }

    // -- all_verbs / extract_verbs / compress_verbs -------------------------

    #[test]
    fn test_all_verbs_count() {
        assert_eq!(all_verbs().len(), 4);
    }

    #[test]
    fn test_extract_verbs() {
        let ev = extract_verbs();
        assert_eq!(ev.len(), 2);
        assert!(ev.contains(&ShellMenuVerb::Extract));
        assert!(ev.contains(&ShellMenuVerb::ExtractHere));
    }

    #[test]
    fn test_compress_verbs() {
        let cv = compress_verbs();
        assert_eq!(cv.len(), 2);
        assert!(cv.contains(&ShellMenuVerb::CompressZip));
        assert!(cv.contains(&ShellMenuVerb::Compress));
    }

    // -- archive extensions -------------------------------------------------

    #[test]
    fn test_archive_exts_not_empty() {
        assert!(!ARCHIVE_EXTS.is_empty());
        for ext in ARCHIVE_EXTS {
            assert!(
                ext.starts_with('.'),
                "extension {ext:?} must start with a dot"
            );
        }
    }

    // -- sentinel constants -------------------------------------------------

    #[test]
    fn test_sentinel_constants_non_empty() {
        assert!(!SENTINEL_KEY.is_empty());
        assert!(!SENTINEL_VALUE.is_empty());
        assert_eq!(SENTINEL_DATA, "1");
    }

    // -- get_shell_menu_state -----------------------------------------------

    #[test]
    fn test_get_shell_menu_state_non_windows() {
        let state = get_shell_menu_state();
        if cfg!(target_os = "windows") {
            assert!(state.supported);
        } else {
            assert!(!state.supported);
        }
        assert_eq!(state.archive_extensions.len(), ARCHIVE_EXTS.len());
    }

    // -- HRESULT not-found helper (Windows only) ---------------------------

    #[test]
    #[cfg(target_os = "windows")]
    fn test_is_not_found_constant_is_correct() {
        // Verify that HR_FILE_NOT_FOUND matches HRESULT::from_win32(2).
        // ERROR_FILE_NOT_FOUND = 2, and HRESULT::from_win32(2) = (2 | 0x80070000) = 0x80070002.
        assert_eq!(platform::HR_FILE_NOT_FOUND, 0x80070002u32 as i32);
    }
}
