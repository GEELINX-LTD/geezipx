//! Confined, transactional output for archive extraction.
//!
//! All entry paths are resolved relative to an open directory capability. A
//! failed or cancelled writer never replaces an existing destination file.
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cap_std::fs::{Dir, OpenOptions};

use super::{is_entry_path_dangerous, normalize_path};
use crate::{GeeZipError, GeeZipResult};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// An extraction destination. Entry operations cannot follow links outside it.
pub struct ExtractionDir {
    root: Dir,
}

impl ExtractionDir {
    /// Create/open the user-selected root. Links in entry paths are rejected.
    pub fn new(dest: &Path) -> GeeZipResult<Self> {
        std::fs::create_dir_all(dest)
            .map_err(|e| GeeZipError::io(e, "creating extraction destination"))?;
        let root = Dir::open_ambient_dir(dest, cap_std::ambient_authority())
            .map_err(|e| GeeZipError::io(e, "opening extraction destination"))?;
        Ok(Self { root })
    }

    fn relative_path(&self, name: &str) -> GeeZipResult<PathBuf> {
        let path = Path::new(name);
        if is_entry_path_dangerous(path) {
            return Err(traversal(name));
        }
        Ok(normalize_path(path))
    }

    fn reject_link(&self, path: &Path, name: &str) -> GeeZipResult<()> {
        match self.root.symlink_metadata(path) {
            Ok(metadata) => {
                let is_link = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let is_link = {
                    use cap_std::fs::MetadataExt;
                    is_link || metadata.file_attributes() & 0x400 != 0
                };
                if is_link {
                    return Err(traversal(name));
                }
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(GeeZipError::io(e, "checking extraction path")),
        }
    }

    /// Create a directory under the root, rejecting symlinks and reparse points.
    pub fn create_dir(&self, name: &str) -> GeeZipResult<()> {
        let relative = self.relative_path(name)?;
        let mut prefix = PathBuf::new();
        for component in relative.components() {
            if let Component::Normal(part) = component {
                prefix.push(part);
                self.reject_link(&prefix, name)?;
                match self.root.create_dir(&prefix) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(GeeZipError::io(e, "creating entry directory")),
                }
                // The capability operations also enforce confinement if a
                // component is replaced between this check and the operation.
                self.reject_link(&prefix, name)?;
                self.root
                    .open_dir(&prefix)
                    .map_err(|e| GeeZipError::io(e, "opening entry directory"))?;
            }
        }
        Ok(())
    }

    /// Stream an entry to a temporary file, publishing it only after success.
    pub fn write_file(
        &self,
        name: &str,
        overwrite: bool,
        write: impl FnOnce(&mut dyn Write) -> GeeZipResult<u64>,
    ) -> GeeZipResult<u64> {
        let relative = self.relative_path(name)?;
        let leaf = relative.file_name().ok_or_else(|| traversal(name))?;
        let parent_path = relative.parent().unwrap_or(Path::new(""));
        self.create_dir(&parent_path.to_string_lossy())?;
        self.reject_link(&relative, name)?;
        let parent = self
            .root
            .open_dir(if parent_path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent_path
            })
            .map_err(|e| GeeZipError::io(e, "opening output parent"))?;
        if !overwrite && parent.symlink_metadata(leaf).is_ok() {
            return Err(GeeZipError::clobber_denied(name));
        }

        let (temp_name, mut file) = loop {
            let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let temp_name = format!(".geezipx-{}-{serial}.tmp", std::process::id());
            match parent.open_with(&temp_name, OpenOptions::new().write(true).create_new(true)) {
                Ok(file) => break (temp_name, file),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(GeeZipError::io(e, "creating temporary output")),
            }
        };
        let pending = PendingFile {
            parent: &parent,
            name: &temp_name,
        };
        let bytes = write(&mut file)?;
        file.flush()
            .map_err(|e| GeeZipError::io(e, "flushing extracted data"))?;
        drop(file);

        if overwrite {
            // Rename replaces the directory entry itself, never its symlink
            // target, and leaves the old file intact if publication fails.
            parent.rename(&temp_name, &parent, leaf)
        } else {
            // Linking publishes without replacing a destination that appeared
            // while the entry was being decompressed.
            match parent.hard_link(&temp_name, &parent, leaf) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
                Err(_) => {
                    // FAT and some network filesystems cannot create hard
                    // links. Exclusive creation still protects existing files;
                    // remove the new output if copying fails.
                    let mut output = parent
                        .open_with(leaf, OpenOptions::new().write(true).create_new(true))
                        .map_err(|e| {
                            if e.kind() == io::ErrorKind::AlreadyExists {
                                GeeZipError::clobber_denied(name)
                            } else {
                                GeeZipError::io(e, "creating extracted file")
                            }
                        })?;
                    let result = (|| {
                        let mut input = parent.open(&temp_name)?;
                        io::copy(&mut input, &mut output)?;
                        output.flush()
                    })();
                    drop(output);
                    if result.is_err() {
                        let _ = parent.remove_file(leaf);
                    }
                    result
                }
            }
        }
        .map_err(|e| {
            if !overwrite && e.kind() == io::ErrorKind::AlreadyExists {
                GeeZipError::clobber_denied(name)
            } else {
                GeeZipError::io(e, "publishing extracted file")
            }
        })?;
        drop(pending);
        Ok(bytes)
    }
}

struct PendingFile<'a> {
    parent: &'a Dir,
    name: &'a str,
}

impl Drop for PendingFile<'_> {
    fn drop(&mut self) {
        let _ = self.parent.remove_file(self.name);
    }
}

fn traversal(name: &str) -> GeeZipError {
    GeeZipError::PathTraversal {
        entry: name.to_owned(),
        target: "extraction destination".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_drive_relative_paths_before_creating_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = ExtractionDir::new(dir.path()).unwrap();
        for name in ["C:evil.txt", "c:folder/evil.txt", "Z:", "C:/evil.txt"] {
            assert!(matches!(
                output.write_file(name, true, |_| panic!("unsafe entry must not be read")),
                Err(GeeZipError::PathTraversal { .. })
            ));
            assert!(matches!(
                output.create_dir(name),
                Err(GeeZipError::PathTraversal { .. })
            ));
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn failed_and_cancelled_writes_preserve_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("file.txt");
        std::fs::write(&target, b"original").unwrap();
        let output = ExtractionDir::new(dir.path()).unwrap();
        for cancelled in [false, true] {
            let result = output.write_file("file.txt", true, |writer| {
                writer.write_all(b"partial")?;
                Err(if cancelled {
                    GeeZipError::Cancelled
                } else {
                    GeeZipError::io(io::Error::other("broken archive"), "extracting")
                })
            });
            assert!(result.is_err());
            assert_eq!(std::fs::read(&target).unwrap(), b"original");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn successful_write_replaces_and_creates_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let output = ExtractionDir::new(dir.path()).unwrap();
        for overwrite in [false, true] {
            output
                .write_file("nested/文件.txt", overwrite, |writer| {
                    writer.write_all(b"complete")?;
                    Ok(8)
                })
                .unwrap();
        }
        assert_eq!(
            std::fs::read(dir.path().join("nested/文件.txt")).unwrap(),
            b"complete"
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("nested"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn no_clobber_checks_at_publication_time() {
        let dir = tempfile::tempdir().unwrap();
        let output = ExtractionDir::new(dir.path()).unwrap();
        let target = dir.path().join("race.txt");
        let result = output.write_file("race.txt", false, |writer| {
            writer.write_all(b"archive")?;
            std::fs::write(&target, b"other process")?;
            Ok(7)
        });
        assert!(matches!(result, Err(GeeZipError::ClobberDenied { .. })));
        assert_eq!(std::fs::read(target).unwrap(), b"other process");
    }

    #[test]
    fn failed_new_file_leaves_no_output_or_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let output = ExtractionDir::new(dir.path()).unwrap();
        assert!(output
            .write_file("new.txt", false, |writer| {
                writer.write_all(b"partial")?;
                Err(GeeZipError::Cancelled)
            })
            .is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(output.create_dir("../escape").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_parents_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("victim.txt");
        std::fs::write(&victim, b"untouched").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join("file-link")).unwrap();
        let output = ExtractionDir::new(dir.path()).unwrap();
        for name in ["link/victim.txt", "file-link"] {
            assert!(output
                .write_file(name, true, |writer| {
                    writer.write_all(b"bad")?;
                    Ok(3)
                })
                .is_err());
        }
        assert!(output.create_dir("link/new").is_err());
        assert_eq!(std::fs::read(victim).unwrap(), b"untouched");
        assert!(!outside.path().join("new").exists());
    }
}
