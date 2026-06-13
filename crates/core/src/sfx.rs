//! Self-extracting archive (SFX) creation.
//!
//! Supports creating ZIP SFX executables by concatenating a pre-compiled
//! platform-native stub binary with a ZIP archive payload.
//!
//! The stub binary scans itself for the ZIP data at runtime and extracts
//! all files to the current or specified output directory.

use crate::error::GeeZipResult;

/// Target platform for SFX creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfxTarget {
    /// Linux x86_64 ELF binary.
    Linux,
    /// Windows x86_64 PE executable (.exe).
    Windows,
    /// macOS x86_64 Mach-O binary.
    MacOS,
}

impl SfxTarget {
    /// Detect the host platform at compile time.
    pub fn host() -> Self {
        if cfg!(target_os = "linux") {
            SfxTarget::Linux
        } else if cfg!(target_os = "windows") {
            SfxTarget::Windows
        } else {
            SfxTarget::MacOS
        }
    }

    /// Recommended output file extension for this target.
    pub fn extension(&self) -> &'static str {
        match self {
            SfxTarget::Linux | SfxTarget::MacOS => "",
            SfxTarget::Windows => ".exe",
        }
    }
}

/// Return the pre-compiled stub bytes for the given target platform.
///
/// The stub binaries are embedded at compile time via `include_bytes!`.
fn get_stub(target: SfxTarget) -> Option<&'static [u8]> {
    match target {
        SfxTarget::Linux => Some(include_bytes!("../stubs/linux-x86_64/sfx-stub")),
        SfxTarget::Windows => Some(include_bytes!("../stubs/windows-x86_64/sfx-stub.exe")),
        SfxTarget::MacOS => Some(include_bytes!("../stubs/macos-x86_64/sfx-stub")),
    }
}

/// Create a ZIP SFX by concatenating the platform stub with ZIP archive data.
///
/// The approach is simple concatenation: `[stub_bytes] + [zip_bytes]`.
/// The stub binary scans itself at runtime to locate the ZIP payload boundary
/// via the End-of-Central-Directory record, so no ZIP offset fixing is needed.
pub fn create_zip_sfx(zip_data: &[u8], target: SfxTarget) -> GeeZipResult<Vec<u8>> {
    let stub = get_stub(target).ok_or_else(|| crate::error::GeeZipError::Format {
        message: format!(
            "SFX stub for {:?} is not embedded in this build. \
                 Build the sfx-stub crate and copy the binary to crates/core/stubs/.",
            target
        ),
        format: crate::detect::ArchiveFormat::Zip,
    })?;

    let mut sfx = Vec::with_capacity(stub.len() + zip_data.len());
    sfx.extend_from_slice(stub);
    sfx.extend_from_slice(zip_data);
    Ok(sfx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn create_test_zip() -> Vec<u8> {
        use crate::archive::zip::ZipWriter;
        use crate::archive::ArchiveWriter;

        let buf = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(buf);
        writer
            .add_entry_from_reader(
                std::path::Path::new("hello.txt"),
                &mut Cursor::new(b"hello sfx"),
            )
            .unwrap();
        let (_, buf) = writer.finalize().unwrap();
        buf.into_inner()
    }

    #[test]
    fn create_sfx_for_host_target() {
        let zip = create_test_zip();
        let sfx = create_zip_sfx(&zip, SfxTarget::host()).unwrap();
        assert!(sfx.len() > zip.len());
        assert!(sfx.ends_with(&zip));
    }

    #[test]
    fn sfx_target_extension() {
        assert_eq!(SfxTarget::Windows.extension(), ".exe");
        assert_eq!(SfxTarget::Linux.extension(), "");
        assert_eq!(SfxTarget::MacOS.extension(), "");
    }

    #[test]
    fn host_detection_does_not_panic() {
        let _host = SfxTarget::host();
    }
}
