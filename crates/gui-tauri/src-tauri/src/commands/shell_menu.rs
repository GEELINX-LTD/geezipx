//! Windows Shell context menu (right-click) management commands.
//!
//! Lets the user enable / disable individual Explorer right-click verbs
//! from the Settings page.  The runtime writes directly to
//! `HKCU\Software\Classes` via the [`windows-registry`] safe API so no
//! administrator privileges are required and no external `reg.exe` process
//! is spawned.
//!
//! Sub-menu structure (v0.7.6+)
//! -----------------------------
//! Four verb entries are grouped under a parent `GeeZipX` key using
//! nested `shell` sub-keys, producing a fly-out sub-menu:
//!
//! ```text
//! SystemFileAssociations\.zip\shell\GeeZipX          ← parent (MUIVerb, Icon)
//! SystemFileAssociations\.zip\shell\GeeZipX\shell\Extract   ← child (MUIVerb, command)
//! SystemFileAssociations\.zip\shell\GeeZipX\shell\ExtractHere
//! ```
//!
//! Child keys carry `MUIVerb` and `command`; the parent holds `Icon`
//! and `MUIVerb`.  No `SubCommands` — Explorer uses nested `shell` keys
//! to render cascaded menus for static verbs.
//!
//! For `*` / `Directory` the nested compress verbs follow the same pattern:
//! `*\shell\GeeZipX\shell\CompressZip` etc.
//!
//! i18n
//! ----
//! The frontend passes the current locale (`"zh-CN"` or `"en"`) when saving
//! settings.  Menu labels (`MUIVerb`) are written in the corresponding
//! language; the parent label is always "GeeZipX" regardless of locale.
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

/// Build the HKCU-relative registry key path for an extract **parent** key
/// on a given extension.
///
/// Example: `reg_parent_key_for_ext(".zip")` →
/// `Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX`
pub fn reg_parent_key_for_ext(ext: &str) -> String {
    format!(r"Software\Classes\SystemFileAssociations\{ext}\shell\GeeZipX")
}

/// Build the HKCU-relative registry key path for the compress **parent** key
/// on `*` (all files).
pub fn reg_parent_key_for_any_file() -> String {
    r"Software\Classes\*\shell\GeeZipX".to_string()
}

/// Build the HKCU-relative registry key path for the compress **parent** key
/// on `Directory`.
pub fn reg_parent_key_for_dir() -> String {
    r"Software\Classes\Directory\shell\GeeZipX".to_string()
}

/// Build the HKCU-relative registry key path for a **nested child** extract
/// verb for a given extension. Explorer renders children nested under the
/// parent `GeeZipX` key as a cascaded sub-menu.
///
/// Example: `reg_key_for_ext(".zip", ShellMenuVerb::Extract)` →
/// `Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX\shell\Extract`
pub fn reg_key_for_ext(ext: &str, verb: ShellMenuVerb) -> String {
    let name = verb_key_name(verb);
    format!(r"Software\Classes\SystemFileAssociations\{ext}\shell\GeeZipX\shell\{name}")
}

/// Build the HKCU-relative registry key path for a **nested child** compress
/// verb on `*` (all files).
pub fn reg_key_for_any_file(verb: ShellMenuVerb) -> String {
    format!(
        r"Software\Classes\*\shell\GeeZipX\shell\{}",
        verb_key_name(verb)
    )
}

/// Build the HKCU-relative registry key path for a **nested child** compress
/// verb on `Directory`.
pub fn reg_key_for_dir(verb: ShellMenuVerb) -> String {
    format!(
        r"Software\Classes\Directory\shell\GeeZipX\shell\{}",
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

/// Localized display label for a verb (used as the MUIVerb value on child
/// verb keys).  Falls back to English for unknown locales.
pub fn verb_label(verb: ShellMenuVerb, locale: &str) -> &'static str {
    match locale {
        "zh-CN" => match verb {
            ShellMenuVerb::Extract => "解压缩到...",
            ShellMenuVerb::ExtractHere => "解压缩到当前文件夹",
            ShellMenuVerb::CompressZip => "压缩为 ZIP",
            ShellMenuVerb::Compress => "压缩为...",
        },
        _ => match verb {
            ShellMenuVerb::Extract => "Extract to...",
            ShellMenuVerb::ExtractHere => "Extract here",
            ShellMenuVerb::CompressZip => "Compress as ZIP",
            ShellMenuVerb::Compress => "Compress as...",
        },
    }
}

/// Localized display label for the parent sub-menu key.  Currently the same
/// in all locales ("GeeZipX"), but provided as a function for extensibility.
pub fn parent_label(_locale: &str) -> &'static str {
    "GeeZipX"
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
/// - `locale`: current UI language (`"zh-CN"` or `"en"`) for localized MUIVerb labels.
///
/// On Windows the function writes / deletes registry keys under
/// `HKCU\Software\Classes`, calls `SHChangeNotify` to refresh Explorer, and
/// writes a sentinel so the NSIS installer preserves the user's choice on
/// upgrade.
///
/// On other platforms it returns `Err("unsupported platform")`.
#[tauri::command]
pub fn set_shell_menu(enabled: bool, verbs: Vec<String>, locale: String) -> Result<(), String> {
    set_shell_menu_impl(enabled, verbs, &locale)
}

#[cfg(target_os = "windows")]
fn set_shell_menu_impl(enabled: bool, verbs: Vec<String>, locale: &str) -> Result<(), String> {
    let parsed: Vec<ShellMenuVerb> = verbs.iter().filter_map(|v| parse_verb(v)).collect();

    if enabled {
        platform::register_verbs(&parsed, locale)?;
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
fn set_shell_menu_impl(_enabled: bool, _verbs: Vec<String>, _locale: &str) -> Result<(), String> {
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

    // NOTE: `is_not_found` is inlined at call sites because
    // `windows_result::Error` is not publicly nameable in v0.4.x.

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
            Err(e) if e.code().0 == HR_FILE_NOT_FOUND => Ok(false),
            Err(e) => Err(format!("failed to query {key_path}: {e}")),
        }
    }

    /// Recursively delete a registry key tree.  A missing key is treated as
    /// success; any other failure is propagated.
    fn reg_delete_tree(key_path: &str) -> Result<(), String> {
        match CURRENT_USER.remove_tree(key_path) {
            Ok(()) => Ok(()),
            Err(e) if e.code().0 == HR_FILE_NOT_FOUND => Ok(()),
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

    /// Write a single **child** extract verb under one extension.
    ///
    /// Child keys only carry `MUIVerb` and `command`; `Icon` and the default
    /// value live on the parent key (see [`write_parent_key`]).
    fn write_extract_verb(
        ext: &str,
        verb: ShellMenuVerb,
        exe: &str,
        locale: &str,
    ) -> Result<(), String> {
        let root = reg_key_for_ext(ext, verb);
        let label = verb_label(verb, locale);
        let flag = cli_flag_for_verb(verb);
        let cmd = build_command(exe, flag);

        let key = CURRENT_USER
            .create(&root)
            .map_err(|e| format!("failed to create key {root}: {e}"))?;
        key.set_string("MUIVerb", label)
            .map_err(|e| format!("failed to set MUIVerb at {root}: {e}"))?;

        let cmd_key_path = format!("{root}\\command");
        let cmd_key = CURRENT_USER
            .create(&cmd_key_path)
            .map_err(|e| format!("failed to create key {cmd_key_path}: {e}"))?;
        cmd_key
            .set_string("", &cmd)
            .map_err(|e| format!("failed to set default value at {cmd_key_path}: {e}"))?;

        Ok(())
    }

    /// Write a single **child** compress verb for `*` and `Directory`.
    ///
    /// Same child-only pattern as [`write_extract_verb`]: only `MUIVerb` and
    /// `command`, no `Icon` or default value.  `MultiSelectModel` is kept on
    /// the Compress child.
    fn write_compress_verb(verb: ShellMenuVerb, exe: &str, locale: &str) -> Result<(), String> {
        let label = verb_label(verb, locale);
        let flag = cli_flag_for_verb(verb);
        let cmd = build_command(exe, flag);

        for root_path in [reg_key_for_any_file(verb), reg_key_for_dir(verb)] {
            let key = CURRENT_USER
                .create(&root_path)
                .map_err(|e| format!("failed to create key {root_path}: {e}"))?;
            key.set_string("MUIVerb", label)
                .map_err(|e| format!("failed to set MUIVerb at {root_path}: {e}"))?;

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

    /// Write a **parent** key with MUIVerb and Icon (no command, no SubCommands).
    /// Explorer renders nested `shell` children as a cascaded sub-menu.
    fn write_parent_key(key_path: &str, exe: &str, locale: &str) -> Result<(), String> {
        let label = parent_label(locale);
        let key = CURRENT_USER
            .create(key_path)
            .map_err(|e| format!("failed to create parent key {key_path}: {e}"))?;
        key.set_string("MUIVerb", label)
            .map_err(|e| format!("failed to set MUIVerb at {key_path}: {e}"))?;
        key.set_string("Icon", &format!("\"{exe}\",0"))
            .map_err(|e| format!("failed to set Icon at {key_path}: {e}"))?;
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

    /// Register the given set of verbs with nested shell structure.
    ///
    /// Removes all existing GeeZipX verbs first so stale keys from a previous
    /// configuration are cleaned up.
    ///
    /// The remove-then-write sequence is not wrapped in a registry transaction
    /// because `windows-registry` does not expose transaction support for
    /// [`Key::remove_tree`] — the underlying `RegDeleteTreeW` does not accept
    /// a transaction handle.  A partial failure during registration leaves the
    /// shell menu in an intermediate state (some keys removed, not all
    /// re-created), which the user can repair by toggling the setting again.
    pub fn register_verbs(verbs: &[ShellMenuVerb], locale: &str) -> Result<(), String> {
        let exe = our_exe();

        // Start from a clean slate.
        remove_all_verbs()?;

        // --- detect which verb groups are active --------------------------

        let has_extract = verbs
            .iter()
            .any(|v| matches!(v, ShellMenuVerb::Extract | ShellMenuVerb::ExtractHere));
        let has_compress = verbs
            .iter()
            .any(|v| matches!(v, ShellMenuVerb::CompressZip | ShellMenuVerb::Compress));

        // --- write parent keys (MUIVerb + Icon only, no SubCommands) ------

        if has_extract {
            for ext in ARCHIVE_EXTS {
                write_parent_key(&reg_parent_key_for_ext(ext), &exe, locale)?;
            }
        }

        if has_compress {
            write_parent_key(&reg_parent_key_for_any_file(), &exe, locale)?;
            write_parent_key(&reg_parent_key_for_dir(), &exe, locale)?;
        }

        // --- write child verb keys -----------------------------------------

        for &verb in verbs {
            match verb {
                ShellMenuVerb::Extract | ShellMenuVerb::ExtractHere => {
                    for ext in ARCHIVE_EXTS {
                        write_extract_verb(ext, verb, &exe, locale)?;
                    }
                }
                ShellMenuVerb::CompressZip | ShellMenuVerb::Compress => {
                    write_compress_verb(verb, &exe, locale)?;
                }
            }
        }

        Ok(())
    }

    /// Remove ALL GeeZipX shell verb keys (children + parents) from HKCU.
    /// Missing keys are silently skipped; any real failure (e.g. access
    /// denied) is propagated.
    pub fn remove_all_verbs() -> Result<(), String> {
        // Remove parent trees (recursive — also removes all nested children).
        for ext in ARCHIVE_EXTS {
            reg_delete_tree(&reg_parent_key_for_ext(ext))?;
        }
        reg_delete_tree(&reg_parent_key_for_any_file())?;
        reg_delete_tree(&reg_parent_key_for_dir())?;

        // Legacy cleanup: remove old flat sibling keys from pre-nested versions.
        // These have path pattern `...\shell\GeeZipX.Extract` (sibling, not nested).
        for ext in ARCHIVE_EXTS {
            for verb in extract_verbs() {
                let legacy = format!(
                    r"Software\Classes\SystemFileAssociations\{ext}\shell\GeeZipX.{}",
                    verb_key_name(verb)
                );
                reg_delete_tree(&legacy)?;
            }
        }
        for verb in compress_verbs() {
            let legacy_file = format!(r"Software\Classes\*\shell\GeeZipX.{}", verb_key_name(verb));
            let legacy_dir = format!(
                r"Software\Classes\Directory\shell\GeeZipX.{}",
                verb_key_name(verb)
            );
            reg_delete_tree(&legacy_file)?;
            reg_delete_tree(&legacy_dir)?;
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
            r"Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX\shell\Extract"
        );
    }

    #[test]
    fn test_reg_key_for_any_file() {
        assert_eq!(
            reg_key_for_any_file(ShellMenuVerb::CompressZip),
            r"Software\Classes\*\shell\GeeZipX\shell\CompressZip"
        );
    }

    #[test]
    fn test_reg_key_for_dir() {
        assert_eq!(
            reg_key_for_dir(ShellMenuVerb::Compress),
            r"Software\Classes\Directory\shell\GeeZipX\shell\Compress"
        );
    }

    // -- parent key paths ---------------------------------------------------

    #[test]
    fn test_reg_parent_key_for_ext() {
        assert_eq!(
            reg_parent_key_for_ext(".zip"),
            r"Software\Classes\SystemFileAssociations\.zip\shell\GeeZipX"
        );
    }

    #[test]
    fn test_reg_parent_key_for_any_file() {
        assert_eq!(
            reg_parent_key_for_any_file(),
            r"Software\Classes\*\shell\GeeZipX"
        );
    }

    #[test]
    fn test_reg_parent_key_for_dir() {
        assert_eq!(
            reg_parent_key_for_dir(),
            r"Software\Classes\Directory\shell\GeeZipX"
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
            let p = reg_parent_key_for_ext(ext);
            assert!(
                !p.to_lowercase().starts_with("hkcu"),
                "parent path {p:?} must not contain HKCU prefix"
            );
            assert!(
                p.starts_with("Software\\Classes\\"),
                "parent path {p:?} must start with Software\\Classes\\"
            );
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
            !reg_parent_key_for_any_file()
                .to_lowercase()
                .starts_with("hkcu"),
            "parent path must not contain HKCU prefix"
        );
        assert!(
            !reg_parent_key_for_dir().to_lowercase().starts_with("hkcu"),
            "parent path must not contain HKCU prefix"
        );
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

    // -- verb_label (i18n) --------------------------------------------------

    #[test]
    fn test_verb_label_locales() {
        // English
        assert_eq!(verb_label(ShellMenuVerb::Extract, "en"), "Extract to...");
        assert_eq!(verb_label(ShellMenuVerb::ExtractHere, "en"), "Extract here");
        assert_eq!(
            verb_label(ShellMenuVerb::CompressZip, "en"),
            "Compress as ZIP"
        );
        assert_eq!(verb_label(ShellMenuVerb::Compress, "en"), "Compress as...");

        // Chinese
        assert_eq!(verb_label(ShellMenuVerb::Extract, "zh-CN"), "解压缩到...");
        assert_eq!(
            verb_label(ShellMenuVerb::ExtractHere, "zh-CN"),
            "解压缩到当前文件夹"
        );
        assert_eq!(
            verb_label(ShellMenuVerb::CompressZip, "zh-CN"),
            "压缩为 ZIP"
        );
        assert_eq!(verb_label(ShellMenuVerb::Compress, "zh-CN"), "压缩为...");

        // Unknown locale falls back to English
        assert_eq!(verb_label(ShellMenuVerb::Extract, "fr"), "Extract to...");
        assert_eq!(verb_label(ShellMenuVerb::ExtractHere, "de"), "Extract here");
        assert_eq!(
            verb_label(ShellMenuVerb::CompressZip, "ja"),
            "Compress as ZIP"
        );
        assert_eq!(verb_label(ShellMenuVerb::Compress, "es"), "Compress as...");
    }

    // -- parent_label -------------------------------------------------------

    #[test]
    fn test_parent_label() {
        assert_eq!(parent_label("en"), "GeeZipX");
        assert_eq!(parent_label("zh-CN"), "GeeZipX");
        assert_eq!(parent_label("fr"), "GeeZipX");
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
