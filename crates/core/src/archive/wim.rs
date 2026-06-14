//! WIM (Windows Imaging Format) archive reader.
//!
//! Read-only support via `wimlib-imagex` CLI subprocess and FFI extraction.
//! WIM is a file-based disk image format developed by Microsoft,
//! used for Windows deployment images (.wim, .swm).

use std::ffi::{CStr, CString};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::archive::{ArchiveReader, Entry};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

// ---------------------------------------------------------------------------
// FFI declarations for libwim (extraction only)
// ---------------------------------------------------------------------------

type WimStruct = std::ffi::c_void;

#[allow(non_camel_case_types)]
type wimlib_tchar = std::os::raw::c_char;

#[link(name = "wim")]
extern "C" {
    fn wimlib_global_init(init_flags: i32) -> i32;
    fn wimlib_get_error_string(code: i32) -> *const wimlib_tchar;

    fn wimlib_open_wim(
        wim_file: *const wimlib_tchar,
        open_flags: i32,
        wim_ret: *mut *mut WimStruct,
    ) -> i32;
    fn wimlib_free(wim: *mut WimStruct);

    fn wimlib_extract_paths(
        wim: *mut WimStruct,
        image: i32,
        target: *const wimlib_tchar,
        paths: *const *const wimlib_tchar,
        num_paths: usize,
        extract_flags: i32,
    ) -> i32;
}

const WIMLIB_OPEN_FLAG_CHECK_INTEGRITY: i32 = 0x00000001;
const WIMLIB_EXTRACT_FLAG_NO_PRESERVE_DIRTY: i32 = 0x00000100;
const WIMLIB_INIT_FLAG_ASSUME_UTF8: i32 = 0x00000001;
const WIMLIB_INIT_FLAG_DONT_ACQUIRE_PRIVILEGES: i32 = 0x00000020;

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

static WIMLIB_INIT: std::sync::Once = std::sync::Once::new();

fn ensure_wimlib_init() {
    WIMLIB_INIT.call_once(|| unsafe {
        wimlib_global_init(WIMLIB_INIT_FLAG_ASSUME_UTF8 | WIMLIB_INIT_FLAG_DONT_ACQUIRE_PRIVILEGES);
    });
}

fn convert_wimlib_error(code: i32) -> GeeZipError {
    let msg = unsafe {
        let ptr = wimlib_get_error_string(code);
        if ptr.is_null() {
            format!("wimlib error code {}", code)
        } else {
            CStr::from_ptr(ptr).to_string_lossy().to_string()
        }
    };
    GeeZipError::format(msg, ArchiveFormat::Wim)
}

// ---------------------------------------------------------------------------
// WimReader
// ---------------------------------------------------------------------------

/// Read-only WIM image reader backed by wimlib.
///
/// Entry listing uses `wimlib-imagex dir` (CLI subprocess).
/// File extraction uses the libwim FFI (`wimlib_extract_paths`).
pub struct WimReader {
    wim_path: PathBuf,
    entries_cache: Option<Vec<Entry>>,
}

impl fmt::Debug for WimReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WimReader").finish_non_exhaustive()
    }
}

impl WimReader {
    pub fn open(path: &Path) -> GeeZipResult<Self> {
        Ok(Self {
            wim_path: path.to_path_buf(),
            entries_cache: None,
        })
    }

    fn ensure_entries(&mut self) -> GeeZipResult<()> {
        if self.entries_cache.is_some() {
            return Ok(());
        }

        let output = std::process::Command::new("/usr/bin/wimlib-imagex")
            .args(["dir", self.wim_path.to_str().unwrap_or("unknown"), "1"])
            .output()
            .map_err(|e| GeeZipError::io(e, "running wimlib-imagex dir"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GeeZipError::format(
                format!("wimlib-imagex dir failed: {}", stderr.trim()),
                ArchiveFormat::Wim,
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let is_dir = trimmed.ends_with('/');
            let path = if is_dir {
                trimmed.trim_end_matches('/').to_owned()
            } else {
                trimmed.to_owned()
            };
            let path = path.trim_start_matches('/').to_owned();
            if path.is_empty() {
                continue;
            }
            entries.push(Entry {
                path,
                size: 0,
                compressed_size: 0,
                crc32: None,
                is_dir,
                modified: None,
            });
        }

        self.entries_cache = Some(entries);
        Ok(())
    }
}

impl ArchiveReader for WimReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Wim
    }

    fn entries(&mut self) -> GeeZipResult<Vec<Entry>> {
        self.ensure_entries()?;
        Ok(self.entries_cache.as_ref().unwrap().clone())
    }

    fn extract(&mut self, entry: &Entry, writer: &mut dyn Write) -> GeeZipResult<u64> {
        if entry.is_dir {
            return Ok(0);
        }

        ensure_wimlib_init();

        let path_c = CString::new(self.wim_path.to_string_lossy().as_bytes())
            .map_err(|_| GeeZipError::format("WIM path contains null byte", ArchiveFormat::Wim))?;

        let mut wim: *mut WimStruct = std::ptr::null_mut();
        let ret =
            unsafe { wimlib_open_wim(path_c.as_ptr(), WIMLIB_OPEN_FLAG_CHECK_INTEGRITY, &mut wim) };
        if ret != 0 {
            let err = convert_wimlib_error(ret);
            if !wim.is_null() {
                unsafe { wimlib_free(wim) };
            }
            return Err(err);
        }

        let temp_dir = tempfile::TempDir::new()
            .map_err(|e| GeeZipError::io(e, "creating temp dir for WIM extraction"))?;
        let target = CString::new(temp_dir.path().to_string_lossy().as_bytes())
            .map_err(|_| GeeZipError::format("temp path null byte", ArchiveFormat::Wim))?;

        let entry_c = CString::new(entry.path.as_bytes())
            .map_err(|_| GeeZipError::format("WIM path null byte", ArchiveFormat::Wim))?;
        let paths = [entry_c.as_ptr()];

        let ret = unsafe {
            wimlib_extract_paths(
                wim,
                1,
                target.as_ptr(),
                paths.as_ptr(),
                1,
                WIMLIB_EXTRACT_FLAG_NO_PRESERVE_DIRTY,
            )
        };
        unsafe { wimlib_free(wim) };

        if ret != 0 {
            return Err(convert_wimlib_error(ret));
        }

        let extracted_path = temp_dir.path().join(&entry.path);
        let data = std::fs::read(&extracted_path).map_err(|e| {
            GeeZipError::io(e, format!("reading extracted WIM file '{}'", entry.path))
        })?;
        let size = data.len() as u64;
        writer
            .write_all(&data)
            .map_err(|e| GeeZipError::io(e, format!("writing WIM entry '{}'", entry.path)))?;
        Ok(size)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_wim(dir: &Path, output: &Path) -> std::io::Result<()> {
        std::process::Command::new("/usr/bin/wimlib-imagex")
            .args([
                "capture",
                dir.to_str().unwrap(),
                output.to_str().unwrap(),
                "Test",
                "--quiet",
            ])
            .output()?;
        Ok(())
    }

    #[test]
    fn wim_list_and_extract() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::write(temp.path().join("hello.txt"), b"hello wim").unwrap();
        let wim_path = temp.path().join("test.wim");
        build_test_wim(temp.path(), &wim_path).expect("wimlib-imagex should create WIM");

        let mut reader = WimReader::open(&wim_path).expect("should open WIM");
        let entries = reader.entries().expect("should list entries");
        let hello = entries.iter().find(|e| e.path == "hello.txt" && !e.is_dir);
        assert!(hello.is_some(), "should find hello.txt: {entries:?}");

        let mut buf = vec![];
        let size = reader
            .extract(hello.unwrap(), &mut buf)
            .expect("should extract");
        assert_eq!(size, 9);
        assert_eq!(buf, b"hello wim");
    }

    #[test]
    fn wim_trait_object_is_supported() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::write(temp.path().join("data.bin"), b"wim trait").unwrap();
        let wim_path = temp.path().join("trait.wim");
        build_test_wim(temp.path(), &wim_path).unwrap();

        let reader = WimReader::open(&wim_path).unwrap();
        let mut boxed: Box<dyn ArchiveReader> = Box::new(reader);
        let entries = boxed.entries().unwrap();
        assert!(!entries.is_empty());
        assert_eq!(boxed.format(), ArchiveFormat::Wim);
    }

    #[test]
    fn wim_invalid_path_fails() {
        let err = WimReader::open(Path::new("/nonexistent/z.wim"))
            .unwrap()
            .entries()
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("wimlib") || msg.contains("WIM") || msg.contains("No such"),
            "unexpected error: {msg}"
        );
    }
}
