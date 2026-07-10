//! File-type association management commands.
//!
//! Lets the GUI let the user bind archive formats so the OS opens them in
//! GeeZipX by default (i.e. double-clicking a `.zip` opens GeeZipX).
//!
//! Each OS is handled separately because the capabilities differ a lot:
//!
//! - **Linux**: `xdg-mime default` / `xdg-mime query default` (per-user, no root).
//!   Toggling a binding both registers GeeZipX *and* makes it the default.
//! - **macOS** (non-sandboxed): Launch Services
//!   `LSSetDefaultRoleHandlerForContentType` / `LSCopyDefaultRoleHandlerForContentType`.
//!   Toggling makes GeeZipX the default directly.
//! - **Windows**: We register a per-user ProgID + `OpenWithProgids` (no admin) so
//!   GeeZipX shows up in "Open with…" / Default Apps. Windows protects the
//!   `UserChoice` default and provides no supported runtime API to set it, so the
//!   *default* must be confirmed by the user in System Settings — we open the
//!   official `ms-settings:defaultapps?registeredAppUser=GeeZipX` deep link for that.

use crate::state::AppState;
use serde::Serialize;
use std::process::Command;

/// One bindable archive format.
#[derive(Debug, Clone, Serialize)]
pub struct AssocItem {
    /// Primary extension including the dot, e.g. `.zip`.
    pub ext: String,
    /// All associated extensions.
    pub exts: Vec<String>,
    /// MIME type (used by Linux `xdg-mime`).
    pub mime: String,
    /// Display name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Whether GeeZipX is currently the OS default handler.
    /// `None` means the OS does not expose this reliably (Windows).
    pub is_default: Option<bool>,
    /// Whether GeeZipX is registered as a handler at all.
    pub is_registered: bool,
}

/// Top-level result of `get_file_associations`.
#[derive(Debug, Serialize)]
pub struct AssociationsResult {
    /// `"linux"` | `"macos"` | `"windows"` | `"unknown"`.
    pub platform: String,
    /// Whether toggling a binding can make GeeZipX the OS default directly.
    /// When `false` (Windows), the UI should guide the user to System Settings.
    pub can_set_default: bool,
    pub items: Vec<AssocItem>,
}

/// Static table of formats the GUI offers to bind.
struct Def {
    ext: &'static str,
    exts: &'static [&'static str],
    mime: &'static str,
    /// macOS UTI — a system UTI where one exists, otherwise a custom UTI that is
    /// declared via `exportedType` in `tauri.conf.json`.
    uti: &'static str,
    name: &'static str,
    description: &'static str,
}

const DEFS: &[Def] = &[
    Def {
        ext: ".zip",
        exts: &[".zip", ".zipx"],
        mime: "application/zip",
        uti: "public.zip-archive",
        name: "ZIP Archive",
        description: "ZIP / ZIPX Archive",
    },
    Def {
        ext: ".tar",
        exts: &[".tar"],
        mime: "application/x-tar",
        uti: "public.tar-archive",
        name: "TAR Archive",
        description: "TAR Archive",
    },
    Def {
        ext: ".gz",
        exts: &[".gz"],
        mime: "application/gzip",
        uti: "public.gzip",
        name: "GZip File",
        description: "GZip Compressed File",
    },
    Def {
        ext: ".tar.gz",
        exts: &[".tar.gz", ".tgz"],
        mime: "application/gzip",
        uti: "com.geelinx.geezipx.targz",
        name: "TAR.GZ Archive",
        description: "GZip-Compressed TAR Archive",
    },
    Def {
        ext: ".bz2",
        exts: &[".bz2"],
        mime: "application/x-bzip2",
        uti: "public.bzip2",
        name: "BZip2 File",
        description: "BZip2 Compressed File",
    },
    Def {
        ext: ".tar.bz2",
        exts: &[".tar.bz2", ".tbz", ".tbz2"],
        mime: "application/x-bzip2",
        uti: "com.geelinx.geezipx.tarbz2",
        name: "TAR.BZ2 Archive",
        description: "BZip2-Compressed TAR Archive",
    },
    Def {
        ext: ".br",
        exts: &[".br"],
        mime: "application/x-brotli",
        uti: "com.geelinx.geezipx.brotli",
        name: "Brotli File",
        description: "Brotli Compressed File",
    },
    Def {
        ext: ".tar.br",
        exts: &[".tar.br", ".tbr"],
        mime: "application/x-brotli",
        uti: "com.geelinx.geezipx.tarbr",
        name: "TAR.BR Archive",
        description: "Brotli-Compressed TAR Archive",
    },
    Def {
        ext: ".lz4",
        exts: &[".lz4"],
        mime: "application/x-lz4",
        uti: "com.geelinx.geezipx.lz4",
        name: "LZ4 File",
        description: "LZ4 Compressed File",
    },
    Def {
        ext: ".tar.lz4",
        exts: &[".tar.lz4"],
        mime: "application/x-lz4",
        uti: "com.geelinx.geezipx.tarlz4",
        name: "TAR.LZ4 Archive",
        description: "LZ4-Compressed TAR Archive",
    },
    Def {
        ext: ".zst",
        exts: &[".zst"],
        mime: "application/zstd",
        uti: "com.geelinx.geezipx.zstd",
        name: "Zstandard File",
        description: "Zstandard Compressed File",
    },
    Def {
        ext: ".tar.zst",
        exts: &[".tar.zst", ".tzst"],
        mime: "application/zstd",
        uti: "com.geelinx.geezipx.tarzst",
        name: "TAR.ZST Archive",
        description: "Zstandard-Compressed TAR Archive",
    },
    Def {
        ext: ".xz",
        exts: &[".xz"],
        mime: "application/x-xz",
        uti: "com.geelinx.geezipx.xz",
        name: "XZ File",
        description: "XZ Compressed File",
    },
    Def {
        ext: ".tar.xz",
        exts: &[".tar.xz", ".txz"],
        mime: "application/x-xz",
        uti: "com.geelinx.geezipx.tarxz",
        name: "TAR.XZ Archive",
        description: "XZ-Compressed TAR Archive",
    },
    Def {
        ext: ".lzma",
        exts: &[".lzma"],
        mime: "application/x-lzma",
        uti: "com.geelinx.geezipx.lzma",
        name: "LZMA File",
        description: "LZMA Compressed File",
    },
    Def {
        ext: ".lz",
        exts: &[".lz"],
        mime: "application/x-lzip",
        uti: "com.geelinx.geezipx.lz",
        name: "LZ File",
        description: "LZ Compressed File",
    },
    Def {
        ext: ".7z",
        exts: &[".7z"],
        mime: "application/x-7z-compressed",
        uti: "com.geelinx.geezipx.sevenzip",
        name: "7-Zip Archive",
        description: "7-Zip Archive",
    },
    Def {
        ext: ".rar",
        exts: &[".rar"],
        mime: "application/vnd.rar",
        uti: "com.geelinx.geezipx.rar",
        name: "RAR Archive",
        description: "RAR Archive",
    },
    Def {
        ext: ".cab",
        exts: &[".cab"],
        mime: "application/vnd.ms-cab-compressed",
        uti: "com.geelinx.geezipx.cab",
        name: "CAB Archive",
        description: "Microsoft Cabinet Archive",
    },
    Def {
        ext: ".asar",
        exts: &[".asar"],
        mime: "application/octet-stream",
        uti: "com.geelinx.geezipx.asar",
        name: "ASAR Archive",
        description: "Electron ASAR Archive",
    },
    Def {
        ext: ".deb",
        exts: &[".deb"],
        mime: "application/vnd.debian.binary-package",
        uti: "com.geelinx.geezipx.deb",
        name: "Debian Package",
        description: "Debian Package",
    },
    Def {
        ext: ".cpio",
        exts: &[".cpio"],
        mime: "application/x-cpio",
        uti: "com.geelinx.geezipx.cpio",
        name: "CPIO Archive",
        description: "CPIO Archive",
    },
    Def {
        ext: ".iso",
        exts: &[".iso"],
        mime: "application/x-iso9660-image",
        uti: "com.geelinx.geezipx.iso",
        name: "ISO Image",
        description: "ISO 9660 Disc Image",
    },
    Def {
        ext: ".udf",
        exts: &[".udf"],
        mime: "application/x-udf",
        uti: "com.geelinx.geezipx.udf",
        name: "UDF Image",
        description: "UDF Disc Image",
    },
    Def {
        ext: ".lzh",
        exts: &[".lzh", ".lha"],
        mime: "application/x-lzh",
        uti: "com.geelinx.geezipx.lzh",
        name: "LZH Archive",
        description: "LZH / LHA Archive",
    },
];

fn platform_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "unknown"
    }
}

/// Whether toggling a binding can make GeeZipX the OS default directly.
fn can_set_default_for_platform() -> bool {
    // Windows cannot set the default programmatically; the user must confirm in
    // System Settings. Everywhere else we can.
    platform_name() != "windows"
}

/// Return the list of bindable formats and their current OS binding state.
#[tauri::command]
pub fn get_file_associations() -> AssociationsResult {
    let platform = platform_name().to_string();
    let can_set = can_set_default_for_platform();
    let items = DEFS
        .iter()
        .map(|d| {
            let state = platform::query_state(d.mime, d.ext, d.uti);
            AssocItem {
                ext: d.ext.to_string(),
                exts: d.exts.iter().map(|s| s.to_string()).collect(),
                mime: d.mime.to_string(),
                name: d.name.to_string(),
                description: d.description.to_string(),
                is_default: state.is_default,
                is_registered: state.is_registered,
            }
        })
        .collect();
    AssociationsResult {
        platform,
        can_set_default: can_set,
        items,
    }
}

/// Enable or disable GeeZipX as the handler/default for `ext`.
#[tauri::command]
pub fn set_file_association(
    ext: String,
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let def = DEFS
        .iter()
        .find(|d| d.ext == ext)
        .ok_or_else(|| format!("unknown extension: {ext}"))?;
    platform::apply(def.mime, def.ext, def.uti, enabled, &state)
}

/// Open the OS "Default Apps" settings page so the user can finish binding
/// (mainly needed on Windows, where the default cannot be set programmatically).
#[tauri::command]
pub fn open_association_settings(ext: Option<String>) -> Result<(), String> {
    platform::open_settings(ext.as_deref())
}

// ---------------------------------------------------------------------------
// Platform-specific implementations
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub struct AssocState {
        pub is_default: Option<bool>,
        pub is_registered: bool,
    }

    /// Locate our installed `.desktop` file by scanning the standard
    /// application directories for a file that references our bundle identifier
    /// or our executable name. Falls back to `GeeZipX.desktop`.
    fn our_desktop() -> String {
        let exe_name = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "geezipx".into());
        let identifier = "com.geelinx.geezipx";

        let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            format!("{}/.local/share", std::env::var("HOME").unwrap_or_default())
        });
        let dirs = [
            std::path::Path::new(&data_home).join("applications"),
            std::path::Path::new("/usr/local/share/applications").to_path_buf(),
            std::path::Path::new("/usr/share/applications").to_path_buf(),
        ];

        for dir in &dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                        continue;
                    }
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if content.contains(identifier) || content.contains(&exe_name) {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                return name.to_string();
                            }
                        }
                    }
                }
            }
        }
        "GeeZipX.desktop".into()
    }

    fn xdg_mime(args: &[&str]) -> Option<String> {
        let out = Command::new("xdg-mime").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }

    pub fn query_state(mime: &str, _ext: &str, _uti: &str) -> AssocState {
        let desktop = our_desktop();
        let cur = xdg_mime(&["query", "default", mime]).filter(|s| !s.is_empty());
        let is_default = cur.as_ref().map(|c| c.eq_ignore_ascii_case(&desktop));
        AssocState {
            is_default,
            is_registered: is_default.unwrap_or(false),
        }
    }

    pub fn apply(
        mime: &str,
        _ext: &str,
        _uti: &str,
        enabled: bool,
        state: &AppState,
    ) -> Result<(), String> {
        let desktop = our_desktop();
        if enabled {
            // Remember the previous default so we can restore it on unbind.
            if let Some(prev) = xdg_mime(&["query", "default", mime])
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case(&desktop))
            {
                if let Ok(mut backup) = state.assoc_backup.lock() {
                    backup.insert(mime.to_string(), prev);
                }
            }
            let status = Command::new("xdg-mime")
                .args(["default", &desktop, mime])
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("xdg-mime default failed for {mime}"));
            }
        } else if let Some(prev) = state
            .assoc_backup
            .lock()
            .ok()
            .and_then(|mut b| b.remove(mime))
        {
            // Restore the previous default if we recorded one.
            let _ = Command::new("xdg-mime")
                .args(["default", &prev, mime])
                .status();
        }
        Ok(())
    }

    pub fn open_settings(_ext: Option<&str>) -> Result<(), String> {
        // On Linux the toggle already sets the default; there is no single
        // universal "Default Apps" GUI to open, so this is a no-op.
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    use core_foundation::base::OSStatus;
    use core_foundation::string::CFString;
    use core_foundation_sys::array::CFArrayRef;
    use core_foundation_sys::string::CFStringRef;

    pub struct AssocState {
        pub is_default: Option<bool>,
        pub is_registered: bool,
    }

    const ROLE_VIEWER: u32 = 1 << 1;

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn LSCopyDefaultRoleHandlerForContentType(
            contentType: CFStringRef,
            role: u32,
        ) -> CFStringRef;
        fn LSCopyAllRoleHandlersForContentType(contentType: CFStringRef, role: u32) -> CFArrayRef;
        fn LSSetDefaultRoleHandlerForContentType(
            contentType: CFStringRef,
            role: u32,
            handlerBundleID: CFStringRef,
        ) -> OSStatus;
    }

    fn bundle_id() -> String {
        // Must match `identifier` in tauri.conf.json.
        "com.geelinx.geezipx".into()
    }

    pub fn query_state(_mime: &str, _ext: &str, uti: &str) -> AssocState {
        let uti_cf = CFString::new(uti);
        let bid = bundle_id();

        let is_default = unsafe {
            let def =
                LSCopyDefaultRoleHandlerForContentType(uti_cf.as_concrete_TypeRef(), ROLE_VIEWER);
            if def.is_null() {
                Some(false)
            } else {
                let s = CFString::wrap_under_create_rule(def);
                Some(s.to_string() == bid)
            }
        };

        let is_registered = unsafe {
            let all =
                LSCopyAllRoleHandlersForContentType(uti_cf.as_concrete_TypeRef(), ROLE_VIEWER);
            if all.is_null() {
                is_default.unwrap_or(false)
            } else {
                let arr =
                    core_foundation::array::CFArray::<CFStringRef>::wrap_under_create_rule(all);
                (0..arr.len()).any(|i| {
                    let item = unsafe { arr.get_unchecked(i) };
                    let s = CFString::wrap_under_get_rule(*item);
                    s.to_string() == bid
                })
            }
        };

        AssocState {
            is_default,
            is_registered,
        }
    }

    pub fn apply(
        _mime: &str,
        _ext: &str,
        uti: &str,
        enabled: bool,
        _state: &AppState,
    ) -> Result<(), String> {
        let uti_cf = CFString::new(uti);
        let bid = CFString::new(&bundle_id());
        if enabled {
            let status = unsafe {
                LSSetDefaultRoleHandlerForContentType(
                    uti_cf.as_concrete_TypeRef(),
                    ROLE_VIEWER,
                    bid.as_concrete_TypeRef(),
                )
            };
            if status != 0 {
                return Err(format!(
                    "LSSetDefaultRoleHandlerForContentType failed: {status}"
                ));
            }
        } else {
            // There is no supported way to "unset" a default on modern macOS.
            // Best-effort: assigning an empty bundle id is ignored by Launch
            // Services, so the user manages this in System Settings. We treat
            // unbind as a no-op that does not error.
            let _ = unsafe {
                LSSetDefaultRoleHandlerForContentType(
                    uti_cf.as_concrete_TypeRef(),
                    ROLE_VIEWER,
                    CFString::new("").as_concrete_TypeRef(),
                )
            };
        }
        Ok(())
    }

    pub fn open_settings(_ext: Option<&str>) -> Result<(), String> {
        // Best-effort: open the Default Apps pane (macOS Sonoma+).
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.DefaultApps")
            .status();
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::process::ExitStatusExt;

    pub struct AssocState {
        pub is_default: Option<bool>,
        pub is_registered: bool,
    }

    fn our_exe() -> String {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "geezipx.exe".into())
    }

    /// ProgID for an extension, e.g. `.zip` -> `GeeZipX.zip`.
    fn prog_id(ext: &str) -> String {
        format!("GeeZipX{ext}")
    }

    /// Run `reg` via cmd. Returns a dummy failed output if `reg` is unavailable.
    fn reg(args: &[&str]) -> std::process::Output {
        Command::new("cmd")
            .args(["/c", "reg"])
            .args(args)
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: ExitStatusExt::from_raw(1),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
    }

    pub fn query_state(_mime: &str, ext: &str, _uti: &str) -> AssocState {
        let pid = prog_id(ext);
        let out = reg(&[
            "query",
            &format!(r"HKCU\Software\Classes\{ext}\OpenWithProgids"),
            "/v",
            &pid,
        ]);
        let registered =
            out.status.success() && String::from_utf8_lossy(&out.stdout).contains(&pid);
        // Windows does not expose the default handler reliably, so we only
        // report whether we are registered as a handler.
        AssocState {
            is_default: None,
            is_registered: registered,
        }
    }

    pub fn apply(
        _mime: &str,
        ext: &str,
        _uti: &str,
        enabled: bool,
        _state: &AppState,
    ) -> Result<(), String> {
        let pid = prog_id(ext);
        let exe = our_exe();

        if enabled {
            // 1) ProgID -> open command.
            let prog_key = format!(r"HKCU\Software\Classes\{pid}");
            let _ = reg(&[
                "add",
                &prog_key,
                "/ve",
                "/t",
                "REG_SZ",
                "/d",
                "GeeZipX Archive",
                "/f",
            ]);
            let cmd_key = format!(r"{prog_key}\shell\open\command");
            let cmd = format!("\"{exe}\" \"%1\"");
            let _ = reg(&["add", &cmd_key, "/ve", "/t", "REG_SZ", "/d", &cmd, "/f"]);

            // 2) Register as an "Open with…" handler for the extension.
            let ow_key = format!(r"HKCU\Software\Classes\{ext}\OpenWithProgids");
            let _ = reg(&["add", &ow_key, "/v", &pid, "/t", "REG_SZ", "/d", "", "/f"]);

            // 3) Register in Default Apps so `ms-settings:defaultapps` finds us.
            let cap_key = r"HKCU\Software\Classes\Applications\geezipx.exe\Capabilities";
            let _ = reg(&[
                "add",
                &format!(r"{cap_key}\FileAssociations"),
                "/v",
                ext,
                "/t",
                "REG_SZ",
                "/d",
                &pid,
                "/f",
            ]);
            let _ = reg(&[
                "add",
                cap_key,
                "/v",
                "ApplicationName",
                "/t",
                "REG_SZ",
                "/d",
                "GeeZipX",
                "/f",
            ]);
            let _ = reg(&[
                "add",
                cap_key,
                "/v",
                "ApplicationDescription",
                "/t",
                "REG_SZ",
                "/d",
                "GeeZipX Archive Tool",
                "/f",
            ]);
            let _ = reg(&[
                "add",
                r"HKCU\Software\RegisteredApplications",
                "/v",
                "GeeZipX",
                "/t",
                "REG_SZ",
                "/d",
                cap_key,
                "/f",
            ]);
        } else {
            // Remove the handler registration (best-effort, ignore errors).
            let ow_key = format!(r"HKCU\Software\Classes\{ext}\OpenWithProgids");
            let _ = reg(&["delete", &ow_key, "/v", &pid, "/f"]);
            let _ = reg(&["delete", &format!(r"HKCU\Software\Classes\{pid}"), "/f"]);
        }
        Ok(())
    }

    pub fn open_settings(_ext: Option<&str>) -> Result<(), String> {
        // Official, supported deep link that scrolls to our app in Default Apps
        // (Windows 11 2023-04+). Falls back to the generic page.
        let url = "ms-settings:defaultapps?registeredAppUser=GeeZipX";
        let status = Command::new("cmd")
            .args(["/c", "start", "", &format("\"{url}\"")])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            let _ = Command::new("cmd")
                .args(["/c", "start", "", "ms-settings:defaultapps"])
                .status();
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;

    pub struct AssocState {
        pub is_default: Option<bool>,
        pub is_registered: bool,
    }

    pub fn query_state(_mime: &str, _ext: &str, _uti: &str) -> AssocState {
        AssocState {
            is_default: None,
            is_registered: false,
        }
    }

    pub fn apply(
        _mime: &str,
        _ext: &str,
        _uti: &str,
        _enabled: bool,
        _state: &AppState,
    ) -> Result<(), String> {
        Err("file associations are not supported on this platform".into())
    }

    pub fn open_settings(_ext: Option<&str>) -> Result<(), String> {
        Err("file associations are not supported on this platform".into())
    }
}
