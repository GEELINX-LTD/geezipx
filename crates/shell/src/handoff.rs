//! Bounded UTF-16 handoff for Explorer selections, including unpaired surrogates.
use crate::Action;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
const LIMIT: u64 = 16 * 1024 * 1024;
const MAX_PATHS: usize = 100_000;
fn encode(action: Action, paths: &[Vec<u16>]) -> Result<Vec<u8>, String> {
    let tag = match action {
        Action::Compress => 1,
        Action::CompressZip => 2,
        Action::Extract => 3,
        Action::ExtractHere => 4,
        Action::ExtractFolder => 5,
        Action::Open => return Err("Unsupported handoff action".into()),
    };
    if paths.is_empty() || paths.len() > MAX_PATHS || (action.is_extraction() && paths.len() != 1) {
        return Err("Invalid selection size".into());
    }
    let mut bytes = b"GXSA\x01".to_vec();
    bytes.push(tag);
    bytes.extend_from_slice(&(paths.len() as u32).to_le_bytes());
    for path in paths {
        if path.is_empty() || path.len() > 32_768 || path.contains(&0) {
            return Err("Invalid selection path".into());
        }
        if bytes.len() + 4 + path.len() * 2 > LIMIT as usize {
            return Err("Selection is too large".into());
        }
        bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
        bytes.extend(path.iter().flat_map(|c| c.to_le_bytes()));
    }
    Ok(bytes)
}
fn decode(bytes: &[u8]) -> Result<(Action, Vec<Vec<u16>>), String> {
    if bytes.len() as u64 > LIMIT || bytes.len() < 10 || &bytes[..5] != b"GXSA\x01" {
        return Err("Invalid action file header".into());
    }
    let action = match bytes[5] {
        1 => Action::Compress,
        2 => Action::CompressZip,
        3 => Action::Extract,
        4 => Action::ExtractHere,
        5 => Action::ExtractFolder,
        _ => return Err("Invalid action".into()),
    };
    let mut rest = &bytes[6..];
    fn length(rest: &mut &[u8]) -> Result<usize, String> {
        let (bytes, remaining) = rest.split_at_checked(4).ok_or("Truncated action file")?;
        *rest = remaining;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
    }
    let count = length(&mut rest)?;
    if count == 0
        || count > MAX_PATHS
        || count > rest.len() / 6
        || (action.is_extraction() && count != 1)
    {
        return Err("Invalid selection size".into());
    }
    let mut paths = Vec::with_capacity(count);
    for _ in 0..count {
        let len = length(&mut rest)?;
        if len == 0 || len > 32_768 {
            return Err("Invalid path length".into());
        }
        let (bytes, remaining) = rest.split_at_checked(len * 2).ok_or("Truncated path")?;
        rest = remaining;
        let path: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        if path.contains(&0) {
            return Err("NUL in path".into());
        }
        paths.push(path);
    }
    if !rest.is_empty() {
        return Err("Trailing action file data".into());
    }
    Ok((action, paths))
}
#[cfg(windows)]
fn directory() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("GeeZipXNative/ShellActions"))
        .ok_or("LOCALAPPDATA is unset".into())
}
#[cfg(windows)]
pub fn write(action: Action, paths: &[PathBuf]) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStrExt;
    let paths = paths
        .iter()
        .map(|p| p.as_os_str().encode_wide().collect())
        .collect::<Vec<_>>();
    write_in(&directory()?, &encode(action, &paths)?)
}
#[cfg(any(windows, test))]
fn write_in(directory: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut file = tempfile::Builder::new()
        .prefix("selection-")
        .suffix(".gzsa")
        .tempfile_in(directory)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .map_err(|e| e.to_string())?;
    file.into_temp_path().keep().map_err(|e| e.to_string())
}
fn read_in(path: &Path, directory: &Path) -> Result<(Action, Vec<Vec<u16>>), String> {
    let expected = directory.canonicalize().map_err(|e| e.to_string())?;
    let actual = path.canonicalize().map_err(|e| e.to_string())?;
    if actual.parent() != Some(expected.as_path())
        || actual.extension().is_none_or(|ext| ext != "gzsa")
        || std::fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("Action file must be inside the ShellActions directory".into());
    }
    let file = std::fs::File::open(&actual).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Action file is not a regular file".into());
    }
    let mut bytes = Vec::new();
    (&file)
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let decoded = decode(&bytes)?;
    drop(file);
    std::fs::remove_file(actual).map_err(|e| e.to_string())?;
    Ok(decoded)
}
#[cfg(windows)]
pub fn read(path: &Path) -> Result<(Action, Vec<PathBuf>), String> {
    use std::os::windows::ffi::OsStringExt;
    let (action, paths) = read_in(path, &directory()?)?;
    Ok((
        action,
        paths
            .into_iter()
            .map(|p| std::ffi::OsString::from_wide(&p).into())
            .collect(),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_preserves_utf16_large_selection_and_consumes_once() {
        let root = tempfile::tempdir().unwrap();
        let paths = vec![vec![0x4e2d, 0xd800, 0x20, 0x61]; 1024];
        let bytes = encode(Action::CompressZip, &paths).unwrap();
        let path = write_in(root.path(), &bytes).unwrap();
        assert_eq!(
            read_in(&path, root.path()).unwrap(),
            (Action::CompressZip, paths)
        );
        assert!(!path.exists());
        assert!(read_in(&path, root.path()).is_err());
    }
    #[test]
    fn extraction_round_trip_enforces_one_archive() {
        for action in [Action::Extract, Action::ExtractHere, Action::ExtractFolder] {
            let paths = vec![vec![65, 0xd800, 66]];
            assert_eq!(
                decode(&encode(action, &paths).unwrap()).unwrap(),
                (action, paths)
            );
            assert!(encode(action, &[vec![65], vec![66]]).is_err());
            let mut forged = encode(Action::Compress, &[vec![65], vec![66]]).unwrap();
            forged[5] = 3;
            assert!(decode(&forged).is_err());
        }
    }
    #[test]
    fn rejects_malformed_oversize_and_foreign_files() {
        let bytes = encode(Action::Compress, &[vec![65]]).unwrap();
        for len in 0..bytes.len() {
            assert!(decode(&bytes[..len]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        let mut count = bytes.clone();
        count[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&count).is_err());
        assert!(encode(Action::Compress, &[]).is_err());
        assert!(encode(Action::Compress, &[vec![0]]).is_err());
        assert!(decode(&vec![0; LIMIT as usize + 1]).is_err());
        let expected = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = write_in(outside.path(), &bytes).unwrap();
        assert!(read_in(&path, expected.path()).is_err());
        assert!(path.exists());
        let malformed = write_in(expected.path(), b"bad").unwrap();
        assert!(read_in(&malformed, expected.path()).is_err());
        assert!(malformed.exists());
    }
}
