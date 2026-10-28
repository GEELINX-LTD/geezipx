//! GeeZipX SFX stub — minimal self-extracting ZIP runtime.
//!
//! Prepended to a ZIP archive to create a self-extracting executable.
//! At runtime, scans itself for the ZIP End-of-Central-Directory record,
//! then extracts all files to the current or specified output directory.

use std::env;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
    let file = fs::File::open(&exe_path).map_err(|e| format!("cannot open self: {}", e))?;
    extract_sfx(file, Path::new(out_dir))
}

fn extract_sfx<R: Read + Seek>(mut reader: R, dest: &Path) -> Result<usize, String> {
    // Executables contain ZIP magic constants themselves. Give the ZIP reader
    // an exact offset so its forward signature scan cannot select those bytes.
    let offset = zip_offset(&mut reader).map_err(|e| format!("invalid ZIP footer: {e}"))?;
    let config = zip::read::Config {
        archive_offset: zip::read::ArchiveOffset::Known(offset),
    };
    let mut archive =
        zip::ZipArchive::with_config(config, reader).map_err(|e| format!("invalid ZIP: {e}"))?;
    extract_archive(&mut archive, dest)
}

fn zip_offset(reader: &mut (impl Read + Seek)) -> std::io::Result<u64> {
    use std::io::{Error, ErrorKind};
    let invalid = || Error::new(ErrorKind::InvalidData, "inconsistent ZIP footer offsets");
    let end = reader.seek(SeekFrom::End(0))?;
    let start = end.saturating_sub(22 + u16::MAX as u64);
    reader.seek(SeekFrom::Start(start))?;
    let mut tail = vec![0; (end - start) as usize];
    reader.read_exact(&mut tail)?;
    for index in (0..tail.len().saturating_sub(21)).rev() {
        let record = &tail[index..];
        if &record[..4] != b"PK\x05\x06" {
            continue;
        }
        let comment = u16::from_le_bytes(record[20..22].try_into().unwrap()) as usize;
        if index + 22 + comment != tail.len() {
            continue;
        }
        let eocd = start + index as u64;
        let cd_size = u32::from_le_bytes(record[12..16].try_into().unwrap()) as u64;
        let cd_offset = u32::from_le_bytes(record[16..20].try_into().unwrap()) as u64;
        if eocd >= 20 {
            reader.seek(SeekFrom::Start(eocd - 20))?;
            let mut locator = [0; 20];
            reader.read_exact(&mut locator)?;
            if &locator[..4] == b"PK\x06\x07" {
                let relative = u64::from_le_bytes(locator[8..16].try_into().unwrap());
                return zip64_offset(reader, eocd - 20, relative);
            }
        }
        if cd_size == u32::MAX as u64 || cd_offset == u32::MAX as u64 {
            return Err(invalid());
        }
        return eocd
            .checked_sub(cd_size)
            .and_then(|n| n.checked_sub(cd_offset))
            .ok_or_else(invalid);
    }
    Err(Error::new(ErrorKind::InvalidData, "missing ZIP end record"))
}

fn zip64_offset(
    reader: &mut (impl Read + Seek),
    locator: u64,
    relative: u64,
) -> std::io::Result<u64> {
    use std::io::{Error, ErrorKind};
    // ZIP64 has a variable-length extension. Search backwards in bounded
    // windows and validate the record length against its following locator.
    let mut end = locator;
    let mut buffer = [0u8; 4096];
    while end >= 56 {
        let start = end.saturating_sub(buffer.len() as u64);
        let length = (end - start) as usize;
        reader.seek(SeekFrom::Start(start))?;
        reader.read_exact(&mut buffer[..length])?;
        for index in (0..length.saturating_sub(3)).rev() {
            if &buffer[index..index + 4] != b"PK\x06\x06" {
                continue;
            }
            let position = start + index as u64;
            if locator - position < 56 {
                continue;
            }
            reader.seek(SeekFrom::Start(position))?;
            let mut record = [0; 56];
            reader.read_exact(&mut record)?;
            let size = u64::from_le_bytes(record[4..12].try_into().unwrap());
            if size < 44
                || size.checked_add(12).and_then(|n| position.checked_add(n)) != Some(locator)
            {
                continue;
            }
            if let Some(offset) = position.checked_sub(relative) {
                return Ok(offset);
            }
        }
        if start == 0 {
            break;
        }
        end = start + 3;
    }
    Err(Error::new(
        ErrorKind::InvalidData,
        "missing or invalid ZIP64 end record",
    ))
}

/// Interpret both separator styles and reject drive prefixes/alternate data
/// streams on every host, so an SFX has consistent path semantics.
fn safe_entry_path(name: &str) -> Result<PathBuf, String> {
    let normalized = name.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || normalized.contains([':', '\0'])
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        || !path
            .components()
            .any(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe ZIP entry path: {name:?}"));
    }
    Ok(path.to_path_buf())
}

fn extract_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    dest: &Path,
) -> Result<usize, String> {
    fs::create_dir_all(dest)
        .map_err(|e| format!("cannot create output dir '{}': {e}", dest.display()))?;
    // Keep all subsequent filesystem operations relative to this directory
    // handle. cap-std prevents existing symlinks/reparse points from escaping
    // the output directory, including during concurrent filesystem changes.
    let output_dir = cap_std::fs::Dir::open_ambient_dir(dest, cap_std::ambient_authority())
        .map_err(|e| format!("cannot open output dir '{}': {e}", dest.display()))?;

    let total = archive.len();
    for i in 0..total {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("entry {} error: {}", i, e))?;
        let name = entry
            .name()
            .map_err(|e| format!("entry {i} name error: {e}"))?
            .into_owned();
        let out_path = safe_entry_path(&name)?;

        if entry.is_dir() {
            output_dir
                .create_dir_all(&out_path)
                .map_err(|e| format!("create dir '{}': {}", name, e))?;
            eprintln!("  [dir]  {}", name);
        } else {
            if let Some(parent) = out_path
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            {
                output_dir
                    .create_dir_all(parent)
                    .map_err(|e| format!("create parent for '{}': {}", name, e))?;
            }
            write_entry(&output_dir, &out_path, &mut entry)
                .map_err(|e| format!("extract '{}': {}", name, e))?;
            eprintln!("  [file] {} ({} bytes)", name, entry.size());
        }
    }
    Ok(total)
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn write_entry(
    root: &cap_std::fs::Dir,
    path: &Path,
    reader: &mut impl Read,
) -> std::io::Result<()> {
    let parent_path = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = root.open_dir(parent_path)?;
    let leaf = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("missing output filename"))?;
    if parent
        .symlink_metadata(leaf)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(std::io::Error::other("refusing symlink output"));
    }
    let (name, file) = loop {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let name = format!(".geezipx-sfx-{}-{id}.tmp", std::process::id());
        match parent.open_with(
            &name,
            cap_std::fs::OpenOptions::new().write(true).create_new(true),
        ) {
            Ok(file) => break (name, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let pending = PendingFile {
        parent: &parent,
        name: &name,
    };
    // Close the handle before guard cleanup, including on Windows errors.
    let mut file = file;
    std::io::copy(reader, &mut file)?;
    file.flush()?;
    drop(file);
    parent.rename(&name, &parent, leaf)?;
    drop(pending);
    Ok(())
}

struct PendingFile<'a> {
    parent: &'a cap_std::fs::Dir,
    name: &'a str,
}

impl Drop for PendingFile<'_> {
    fn drop(&mut self) {
        let _ = self.parent.remove_file(self.name);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn archive_with_entry(name: &str) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(b"safe data").unwrap();
        zip::ZipArchive::new(writer.finish().unwrap()).unwrap()
    }

    #[test]
    fn extracts_nested_unicode_files() {
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        let mut archive = archive_with_entry("nested/测试.txt");
        assert_eq!(extract_archive(&mut archive, &dest).unwrap(), 1);
        assert_eq!(
            fs::read(dest.join("nested/测试.txt")).unwrap(),
            b"safe data"
        );
        let mut archive = archive_with_entry("root.txt");
        assert_eq!(extract_archive(&mut archive, &dest).unwrap(), 1);
    }

    #[test]
    fn handles_prepended_executable_and_rejects_truncated_zip() {
        let temp = tempfile::tempdir().unwrap();
        let zip = archive_with_entry("file.txt").into_inner().into_inner();
        let mut sfx = vec![0x90; 4096];
        // A real executable embeds these constants as part of the ZIP reader.
        sfx[64..68].copy_from_slice(b"PK\x01\x02");
        sfx.extend_from_slice(&zip);
        assert_eq!(
            extract_sfx(Cursor::new(sfx.clone()), temp.path()).unwrap(),
            1
        );
        assert_eq!(
            fs::read(temp.path().join("file.txt")).unwrap(),
            b"safe data"
        );
        sfx.truncate(sfx.len() - 10);
        assert!(extract_sfx(Cursor::new(sfx), temp.path()).is_err());
    }

    #[test]
    fn extracts_zip64_with_variable_extension_and_embedded_magic() {
        let temp = tempfile::tempdir().unwrap();
        let mut payload = archive_with_entry("file.txt").into_inner().into_inner();
        let end = payload.len() - 22;
        let mut eocd = payload.split_off(end);
        let cd_size = u32::from_le_bytes(eocd[12..16].try_into().unwrap()) as u64;
        let cd_offset = u32::from_le_bytes(eocd[16..20].try_into().unwrap()) as u64;
        let extension = [0u8; 5000];
        payload.extend_from_slice(b"PK\x06\x06");
        payload.extend_from_slice(&(44 + extension.len() as u64).to_le_bytes());
        payload.extend_from_slice(&45u16.to_le_bytes());
        payload.extend_from_slice(&45u16.to_le_bytes());
        payload.extend_from_slice(&[0; 8]); // disk fields
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.extend_from_slice(&cd_size.to_le_bytes());
        payload.extend_from_slice(&cd_offset.to_le_bytes());
        payload.extend_from_slice(&extension);
        payload.extend_from_slice(b"PK\x06\x07");
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&(end as u64).to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        eocd[8..12].fill(0xff);
        eocd[12..20].fill(0xff);
        payload.extend_from_slice(&eocd);
        let mut sfx = vec![0x90; 4096];
        sfx[64..68].copy_from_slice(b"PK\x01\x02");
        sfx.extend_from_slice(&payload);
        assert_eq!(extract_sfx(Cursor::new(sfx), temp.path()).unwrap(), 1);
        assert_eq!(
            fs::read(temp.path().join("file.txt")).unwrap(),
            b"safe data"
        );
    }

    #[test]
    fn crc_failure_preserves_existing_file_and_removes_temporary_output() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("file.txt"), b"original").unwrap();
        let mut bytes = archive_with_entry("file.txt").into_inner().into_inner();
        let start = bytes
            .windows(b"safe data".len())
            .position(|part| part == b"safe data")
            .unwrap();
        bytes[start] ^= 1;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(extract_archive(&mut archive, temp.path()).is_err());
        assert_eq!(fs::read(temp.path().join("file.txt")).unwrap(), b"original");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn rejects_parent_absolute_and_windows_paths() {
        for name in [
            "../escape.txt",
            "/escape.txt",
            "nested/../../escape.txt",
            r"..\escape.txt",
            r"C:\escape.txt",
            r"\\server\share\escape.txt",
            "file.txt:stream",
            "./",
            "",
        ] {
            assert!(safe_entry_path(name).is_err(), "accepted {name:?}");
        }
        assert_eq!(
            safe_entry_path(r"nested\file.txt").unwrap(),
            Path::new("nested/file.txt")
        );
    }

    #[test]
    fn malicious_archive_cannot_create_parent_file() {
        let temp = tempfile::tempdir().unwrap();
        let mut archive = archive_with_entry("../escape.txt");
        assert!(extract_archive(&mut archive, &temp.path().join("output")).is_err());
        assert!(!temp.path().join("escape.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn existing_symlinks_cannot_escape_destination() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let dest = temp.path().join("output");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, b"keep me").unwrap();
        symlink(&outside, dest.join("link")).unwrap();
        symlink(&victim, dest.join("file-link")).unwrap();
        for name in ["link/victim.txt", "file-link"] {
            let mut archive = archive_with_entry(name);
            assert!(extract_archive(&mut archive, &dest).is_err());
            assert_eq!(fs::read(&victim).unwrap(), b"keep me");
        }
    }
}
