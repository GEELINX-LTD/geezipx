//! Per-user desktop integration. All filesystem and OS calls run on workers.
use crate::{i18n::Language, launch::Action, tr};
use std::path::PathBuf;
#[cfg(target_os = "windows")]
pub mod com;
#[cfg(target_os = "windows")]
pub use geezipx_shell::handoff;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod modern;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

pub const APP_ID: &str = "com.geelinx.geezipx.native";
pub struct Format {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub mime: &'static str,
}
macro_rules! formats {
    ($(($name:literal, $mime:literal, [$($ext:literal),+ $(,)?])),+ $(,)?) => {
        pub const FORMATS: &[Format] = &[$(Format { name: $name, mime: $mime, extensions: &[$($ext),+] }),+];
    }
}
formats! {
    ("ZIP", "application/zip", ["zip", "jar", "war", "apk", "ipa", "xpi"]),
    ("ZIPX", "application/x-geezipx-zipx", ["zipx"]),
    ("7z", "application/x-7z-compressed", ["7z"]),
    ("RAR", "application/vnd.rar", ["rar"]),
    ("TAR", "application/x-tar", ["tar"]),
    ("TAR.GZ", "application/x-compressed-tar", ["tar.gz", "tgz"]),
    ("TAR.BZ2", "application/x-bzip-compressed-tar", ["tar.bz2", "tbz", "tbz2"]),
    ("TAR.XZ", "application/x-xz-compressed-tar", ["tar.xz", "txz"]),
    ("TAR.ZST", "application/x-zstd-compressed-tar", ["tar.zst", "tzst"]),
    ("TAR.BR", "application/x-brotli-compressed-tar", ["tar.br"]),
    ("TAR.LZ4", "application/x-lz4-compressed-tar", ["tar.lz4"]),
    ("TAR.LZMA / TAR.LZ", "application/x-lzma-compressed-tar", ["tar.lzma", "tar.lz", "tlz"]),
    ("GZIP", "application/gzip", ["gz", "gzip"]),
    ("BZIP2", "application/x-bzip2", ["bz2", "bzip2"]),
    ("Brotli", "application/x-brotli", ["br"]),
    ("LZ4", "application/x-lz4", ["lz4"]),
    ("XZ", "application/x-xz", ["xz"]),
    ("LZMA", "application/x-lzma", ["lzma"]),
    ("Lzip", "application/x-lzip", ["lz"]),
    ("Zstandard", "application/zstd", ["zst", "zstd"]),
    ("Unix Compress", "application/x-compress", ["z"]),
    ("CAB", "application/vnd.ms-cab-compressed", ["cab"]),
    ("ASAR", "application/x-asar", ["asar"]),
    ("DEB", "application/vnd.debian.binary-package", ["deb"]),
    ("LZH / LHA", "application/x-lha", ["lzh", "lha"]),
    ("PMA", "application/x-pma", ["pma"]),
    ("ISO", "application/x-cd-image", ["iso"]),
    ("UDF", "application/x-udf", ["udf"]),
    ("CPIO", "application/x-cpio", ["cpio"]),
    ("ZPAQ", "application/x-zpaq", ["zpaq"]),
    ("WIM", "application/x-ms-wim", ["wim"]),
    ("ISZ", "application/x-isz", ["isz"]),
    ("ARJ", "application/x-arj", ["arj"]),
    ("ACE", "application/x-ace", ["ace"]),
    ("ARC", "application/x-arc", ["arc"]),
    ("UU / UUE", "text/x-uuencode", ["uu", "uue"]),
    ("XXE", "text/x-xxencode", ["xxe"]),
    ("IMG / IMA / BIN", "application/x-geezipx-disk-image", ["img", "ima", "bin"]),
    ("MSI / OLE", "application/x-ole-storage", ["msi", "ole", "compound"]),
    ("EGG", "application/x-egg", ["egg"]),
    ("PEA", "application/x-pea", ["pea"]),
    ("ALZ", "application/x-alz", ["alz"]),
    ("AES Crypt", "application/x-aescrypt", ["aes"]),
    ("GeeZipX ENC", "application/x-geezipx-encrypted", ["enc"]),
}
impl Format {
    pub fn available(&self) -> bool {
        let ext = self.extensions[0];
        (ext != "rar" || cfg!(feature = "rar"))
            && (ext != "wim" || cfg!(feature = "wim"))
            && (ext != "zpaq" || cfg!(feature = "zpaq"))
    }
}
#[derive(Clone, Debug, Default)]
pub struct Association {
    pub registered: bool,
    pub is_default: bool,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub associations: Vec<Association>,
    pub menu: bool,
}
#[derive(Clone)]
pub enum Operation {
    Inspect,
    Apply {
        selected: Vec<bool>,
        menu: bool,
        language: Language,
    },
    Remove,
    Defaults,
    /// Repair registered formats, optionally changing only the menu choice.
    Repair {
        menu: Option<bool>,
    },
}
pub fn prepare_windows_upgrade() -> Result<(), String> {
    #[cfg(windows)]
    {
        modern::remove()
    }
    #[cfg(not(windows))]
    {
        Err("Shell extension upgrades require Windows".into())
    }
}
pub fn execute(operation: Operation) -> Result<Snapshot, String> {
    match operation {
        Operation::Inspect => {}
        Operation::Apply {
            selected,
            menu,
            language,
        } => {
            if selected.len() != FORMATS.len() {
                return Err("Invalid format selection".into());
            }
            // Explorer/Finder resolve compound names through their final suffix.
            let selected = if cfg!(any(windows, target_os = "macos")) {
                include_outer_formats(selected)
            } else {
                selected
            };
            platform::apply(&selected, menu, language)?;
        }
        Operation::Remove => platform::remove()?,
        Operation::Defaults => platform::open_defaults()?,
        Operation::Repair { menu } => {
            let snapshot = platform::inspect()?;
            return match repair_operation(&snapshot, menu) {
                Some(operation) => execute(operation),
                None => Ok(snapshot),
            };
        }
    }
    platform::inspect()
}
fn repair_operation(snapshot: &Snapshot, menu: Option<bool>) -> Option<Operation> {
    if !snapshot.menu && !snapshot.associations.iter().any(|a| a.registered) && menu != Some(true) {
        return None;
    }
    Some(Operation::Apply {
        selected: snapshot
            .associations
            .iter()
            .map(|a| {
                if cfg!(target_os = "windows") {
                    a.registered
                } else {
                    a.is_default
                }
            })
            .collect(),
        menu: menu.unwrap_or(snapshot.menu),
        language: Language::default(),
    })
}
fn include_outer_formats(mut selected: Vec<bool>) -> Vec<bool> {
    for (format, enabled) in FORMATS.iter().zip(selected.clone()) {
        if !enabled {
            continue;
        }
        for ext in format.extensions {
            if let Some((_, suffix)) = ext.rsplit_once('.') {
                if let Some(index) = FORMATS.iter().position(|f| f.extensions.contains(&suffix)) {
                    selected[index] = true;
                }
            }
        }
    }
    selected
}

pub fn action_label(action: Action, language: Language) -> &'static str {
    match action {
        Action::Open => tr!(language, OpenArchive),
        Action::Compress => tr!(language, ShellCompress),
        Action::CompressZip => tr!(language, ShellCompressZip),
        Action::Extract => tr!(language, ShellExtract),
        Action::ExtractHere => tr!(language, ShellExtractHere),
        Action::ExtractFolder => tr!(language, ShellExtractFolder),
    }
}
pub(crate) fn executable() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn command(command: &mut std::process::Command) -> Result<String, String> {
    let output = command.output().map_err(|e| format!("{command:?}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{command:?}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installer_menu_choice_does_not_register_unselected_formats() {
        let snapshot = Snapshot {
            associations: vec![Association::default(); FORMATS.len()],
            menu: false,
        };
        let Some(Operation::Apply { selected, menu, .. }) = repair_operation(&snapshot, Some(true))
        else {
            panic!("An explicit menu choice must be applied");
        };
        assert!(menu);
        assert_eq!(selected.len(), FORMATS.len());
        assert!(selected.iter().all(|selected| !selected));
        assert!(repair_operation(&snapshot, None).is_none());
        assert!(repair_operation(&snapshot, Some(false)).is_none());
    }
    #[test]
    fn installer_can_disable_an_existing_menu_and_preserve_format_choices() {
        let mut associations = vec![Association::default(); FORMATS.len()];
        associations[0] = Association {
            registered: true,
            is_default: true,
        };
        let snapshot = Snapshot {
            associations,
            menu: true,
        };
        for override_menu in [None, Some(false), Some(true)] {
            let Some(Operation::Apply { selected, menu, .. }) =
                repair_operation(&snapshot, override_menu)
            else {
                panic!("Existing integration must be repaired");
            };
            assert!(selected[0]);
            assert!(selected[1..].iter().all(|selected| !selected));
            assert_eq!(menu, override_menu.unwrap_or(true));
        }
    }
    #[test]
    fn compound_format_selection_includes_the_os_visible_outer_suffix() {
        let index = |extension| {
            FORMATS
                .iter()
                .position(|f| f.extensions.contains(&extension))
                .unwrap()
        };
        let mut selected = vec![false; FORMATS.len()];
        selected[index("tar.gz")] = true;
        selected[index("tar.lzma")] = true;
        let selected = include_outer_formats(selected);
        for ext in ["tar.gz", "gz", "tar.lzma", "lzma", "lz"] {
            assert!(selected[index(ext)], "{ext}");
        }
        assert!(!selected[index("zip")]);
    }
    #[test]
    fn association_extensions_are_unique_and_never_capture_all_executables() {
        let mut seen = std::collections::HashSet::new();
        for format in FORMATS {
            assert!(!format.extensions.is_empty());
            for ext in format.extensions {
                assert!(seen.insert(*ext), "Duplicate extension: {ext}");
                assert!(!matches!(*ext, "exe" | "*"));
            }
        }
    }
}
