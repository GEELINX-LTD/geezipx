//! Self-extracting archive (SFX) creation.
//!
//! Supports creating ZIP SFX executables by concatenating a pre-compiled
//! platform-native stub binary with a ZIP archive payload.
//!
//! The stub binary scans itself for the ZIP data at runtime and extracts
//! all files to the current or specified output directory.

use std::io::{self, Read, Write};

use crate::error::{GeeZipError, GeeZipResult};

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
    let mut sfx = Vec::new();
    write_zip_sfx(zip_data, &mut sfx, target)?;
    Ok(sfx)
}

/// Stream a ZIP payload after its executable stub without buffering the archive.
///
/// Callers publishing files should stage the output until this returns success.
pub fn write_zip_sfx(
    mut zip_data: impl Read,
    mut output: impl Write,
    target: SfxTarget,
) -> GeeZipResult<u64> {
    let stub = get_stub(target)
        .filter(|stub| !stub.is_empty())
        .ok_or_else(|| GeeZipError::Format {
            message: format!(
                "SFX stub for {:?} is not embedded in this build. \
                 Build the sfx-stub crate and copy the binary to crates/core/stubs/.",
                target
            ),
            format: crate::detect::ArchiveFormat::Zip,
        })?;

    output
        .write_all(stub)
        .map_err(|error| GeeZipError::io(error, "writing SFX stub"))?;
    let copied = io::copy(&mut zip_data, &mut output)
        .map_err(|error| GeeZipError::io(error, "writing SFX payload"))?;
    copied.checked_add(stub.len() as u64).ok_or_else(|| {
        GeeZipError::format(
            "SFX output size overflow",
            crate::detect::ArchiveFormat::Zip,
        )
    })
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
    fn streaming_output_matches_vec_api_and_reports_write_failures() {
        let zip = create_test_zip();
        let expected = create_zip_sfx(&zip, SfxTarget::host()).unwrap();
        let mut actual = Vec::new();
        let count = write_zip_sfx(&zip[..], &mut actual, SfxTarget::host()).unwrap();
        assert_eq!(count, expected.len() as u64);
        assert_eq!(actual, expected);
        assert!(write_zip_sfx(
            &zip[..],
            io::Cursor::new(&mut [0u8; 4][..]),
            SfxTarget::host()
        )
        .is_err());
    }

    #[test]
    fn empty_platform_placeholders_are_not_executables() {
        for target in [SfxTarget::Linux, SfxTarget::Windows, SfxTarget::MacOS] {
            if get_stub(target).is_some_and(|stub| stub.is_empty()) {
                assert!(create_zip_sfx(b"payload", target).is_err());
            }
        }
    }

    #[test]
    fn embedded_host_stub_extracts_files_and_confines_paths() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("output");
        let executable = temp.path().join(if cfg!(windows) {
            "archive.exe"
        } else {
            "archive"
        });
        for (name, accepted) in [("nested/hello.txt", true), ("../escaped.txt", false)] {
            let mut writer = ::zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file(name, ::zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"hello sfx").unwrap();
            let payload = writer.finish().unwrap().into_inner();
            std::fs::write(
                &executable,
                create_zip_sfx(&payload, SfxTarget::host()).unwrap(),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
                    .unwrap();
            }
            let output = std::process::Command::new(&executable)
                .arg(&destination)
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                accepted,
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(
            std::fs::read(destination.join("nested/hello.txt")).unwrap(),
            b"hello sfx"
        );
        assert!(!temp.path().join("escaped.txt").exists());
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
