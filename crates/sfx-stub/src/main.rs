//! GeeZipX SFX stub — minimal self-extracting ZIP runtime.
//!
//! Prepended to a ZIP archive to create a self-extracting executable.
//! At runtime, scans itself for the ZIP End-of-Central-Directory record,
//! then extracts all files to the current or specified output directory.

use std::env;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let out_dir = if args.len() > 1 {
        args[1].as_str()
    } else {
        "."
    };

    match run(out_dir) {
        Ok(count) => {
            eprintln!("Extracted {} file(s) to '{}'", count, out_dir);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("SFX error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run(out_dir: &str) -> Result<usize, String> {
    let exe_path = current_exe_path()?;
    let mut file = fs::File::open(&exe_path).map_err(|e| format!("cannot open self: {}", e))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("cannot stat self: {}", e))?
        .len();

    // Locate the ZIP EOCD signature by scanning backwards from EOF.
    let eocd_pos = find_eocd(&mut file, file_len)?;
    file.seek(SeekFrom::Start(eocd_pos))
        .map_err(|e| format!("seek error: {}", e))?;
    let mut eocd = [0u8; 22];
    file.read_exact(&mut eocd)
        .map_err(|e| format!("read EOCD error: {}", e))?;

    // EOCD layout (offsets from signature):
    //   12: central directory size   (u32 LE)
    //   16: central directory offset (u32 LE)
    let cd_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]) as u64;
    let cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]) as u64;

    // zip_start = eocd_pos - cd_size - cd_offset
    let zip_start = eocd_pos - cd_size - cd_offset;
    let zip_len = (file_len - zip_start) as usize;

    file.seek(SeekFrom::Start(zip_start))
        .map_err(|e| format!("seek to ZIP data: {}", e))?;
    let mut zip_data = vec![0u8; zip_len];
    file.read_exact(&mut zip_data)
        .map_err(|e| format!("read ZIP data: {}", e))?;

    // Extract using the zip crate.
    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("invalid ZIP: {}", e))?;

    let dest = Path::new(out_dir);
    if !dest.exists() {
        fs::create_dir_all(dest)
            .map_err(|e| format!("cannot create output dir '{}': {}", out_dir, e))?;
    }

    let total = archive.len();
    for i in 0..total {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("entry {} error: {}", i, e))?;
        let name = entry.name().to_owned();
        let out_path = dest.join(&name);

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| format!("create dir '{}': {}", name, e))?;
            eprintln!("  [dir]  {}", name);
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("create parent for '{}': {}", name, e))?;
            }
            let mut output =
                fs::File::create(&out_path).map_err(|e| format!("create '{}': {}", name, e))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|e| format!("extract '{}': {}", name, e))?;
            eprintln!("  [file] {} ({} bytes)", name, entry.size());
        }
    }
    Ok(total)
}

/// Find the ZIP EOCD signature (`PK\x05\x06`) by scanning backwards
/// from end-of-file. The EOCD record is at most 22 + 65535 (comment) bytes.
fn find_eocd(file: &mut fs::File, file_len: u64) -> Result<u64, String> {
    const EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
    let search_start = file_len.saturating_sub(65535 + 22);
    let search_len = file_len - search_start;

    file.seek(SeekFrom::Start(search_start))
        .map_err(|e| format!("seek error: {}", e))?;
    let mut buf = vec![0u8; search_len as usize];
    file.read_exact(&mut buf)
        .map_err(|e| format!("read error: {}", e))?;

    for i in (0..buf.len().saturating_sub(3)).rev() {
        if buf[i..i + 4] == EOCD_SIG {
            return Ok(search_start + i as u64);
        }
    }
    Err("no ZIP data found in this SFX executable".into())
}

/// Get the current executable path, using platform-specific methods
/// for reliability.
fn current_exe_path() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link("/proc/self/exe")
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("read /proc/self/exe: {}", e))
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("current_exe: {}", e))
    }
}
