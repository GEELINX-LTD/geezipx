//! Shared launch actions and the lightweight Windows Explorer extension.
//! This crate has no archive engine or GUI dependencies.

#[cfg(windows)]
mod explorer;
#[cfg(any(windows, test))]
pub mod handoff;

pub const PACKAGE_NAME: &str = "GeeZipX.Native.Shell";
pub const APPLICATION_ID: &str = "GeeZipXShell";
pub const PUBLISHER: &str = env!("GEEZIPX_SHELL_PUBLISHER");
pub const EXPLORER_CLSID: &str = "{5AD97452-37F8-4738-A102-5B472DF60C20}";
pub const SETTINGS_KEY: &str = r"Software\GeeZipXNative\Shell";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Compress,
    CompressZip,
    Extract,
    ExtractHere,
    ExtractFolder,
}
impl Action {
    pub const MENU: [Self; 5] = [
        Self::Compress,
        Self::CompressZip,
        Self::Extract,
        Self::ExtractHere,
        Self::ExtractFolder,
    ];
    pub fn argument(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Compress => "compress",
            Self::CompressZip => "compress-zip",
            Self::Extract => "extract",
            Self::ExtractHere => "extract-here",
            Self::ExtractFolder => "extract-folder",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        std::iter::once(Self::Open)
            .chain(Self::MENU)
            .find(|action| action.argument() == value)
    }
    pub fn is_extraction(self) -> bool {
        matches!(
            self,
            Self::Extract | Self::ExtractHere | Self::ExtractFolder
        )
    }
}

/// Hand off the entire selection without invoking a command shell.
#[cfg(windows)]
pub fn dispatch(
    exe: &std::path::Path,
    action: Action,
    paths: &[std::path::PathBuf],
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let file = handoff::write(action, paths)?;
    match std::process::Command::new(exe)
        .arg("--shell-action-file")
        .arg(&file)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW; the separate GUI owns all interaction.
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(file);
            Err(error.to_string())
        }
    }
}
