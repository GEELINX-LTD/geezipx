//! WIM writer — creates uncompressed WIM archives.
//!
//! All file data is stored raw (CompressionType::None). The generated
//! archives are fully valid per the WIM spec and can be read by any
//! compliant tool (wimlib, 7-Zip, etc.).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::archive::{is_entry_path_dangerous, ArchiveWriter};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::{ResourceDescriptor, WimHeader};
use ::sha1::{Digest, Sha1};

/// Writer for creating uncompressed WIM (.wim) archives.
pub struct WimWriter<W: Write + Send> {
    writer: Option<W>,
    format: ArchiveFormat,
    entries: Vec<PendingEntry>,
    spool: Option<File>,
    spool_len: u64,
    directories: Vec<String>,
    chunk_size: u32,
    guid: [u8; 16],
}

struct PendingEntry {
    path: String,
    size: u64,
    offset: u64,
    hash: [u8; 20],
}

impl<W: Write + Send> WimWriter<W> {
    /// Create a new WIM writer with a random GUID and default chunk size.
    pub fn new(writer: W) -> Self {
        let mut guid = [0u8; 16];
        // Generate a random GUID (not cryptographically secure, but unique
        // enough for WIM identification purposes).
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let nanos = now.as_nanos();
        for (i, b) in guid.iter_mut().enumerate() {
            *b = ((nanos >> (i * 8)) & 0xFF) as u8;
        }
        // Add some entropy from the nanoseconds
        guid[0] ^= (now.subsec_nanos() & 0xFF) as u8;
        guid[1] ^= ((now.subsec_nanos() >> 8) & 0xFF) as u8;

        WimWriter {
            writer: Some(writer),
            format: ArchiveFormat::Wim,
            entries: Vec::new(),
            spool: None,
            spool_len: 0,
            directories: Vec::new(),
            // Uncompressed WIMs must declare no compression chunk size.
            chunk_size: 0,
            guid,
        }
    }
}

impl<W: Write + Send> ArchiveWriter for WimWriter<W> {
    fn format(&self) -> ArchiveFormat {
        self.format
    }

    fn add_entry_from_reader(&mut self, path: &Path, reader: &mut dyn Read) -> GeeZipResult<()> {
        let path_str = path.to_string_lossy().to_string();
        if is_entry_path_dangerous(path) {
            return Err(GeeZipError::PathTraversal {
                entry: path_str.clone(),
                target: "WIM archive".into(),
            });
        }
        if self.spool.is_none() {
            self.spool = Some(
                tempfile::tempfile().map_err(|e| GeeZipError::io(e, "creating WIM data spool"))?,
            );
        }
        let spool = self.spool.as_mut().expect("spool was initialized");
        // A failed entry may leave uncommitted bytes; retries overwrite them.
        spool
            .seek(SeekFrom::Start(self.spool_len))
            .map_err(|e| GeeZipError::io(e, "seeking WIM data spool"))?;
        let mut hash = Sha1::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(GeeZipError::io(
                        error,
                        format!("reading entry '{path_str}'"),
                    ))
                }
            };
            spool
                .write_all(&buffer[..read])
                .map_err(|e| GeeZipError::io(e, "writing WIM data spool"))?;
            hash.update(&buffer[..read]);
            size = size.checked_add(read as u64).ok_or_else(|| {
                GeeZipError::format("WIM entry size overflow", ArchiveFormat::Wim)
            })?;
        }
        let next_offset = self
            .spool_len
            .checked_add(size)
            .ok_or_else(|| GeeZipError::format("WIM spool size overflow", ArchiveFormat::Wim))?;
        self.entries.push(PendingEntry {
            path: path_str,
            size,
            offset: self.spool_len,
            hash: hash.finalize().into(),
        });
        self.spool_len = next_offset;
        Ok(())
    }

    fn add_directory(&mut self, path: &Path) -> GeeZipResult<()> {
        let path_str = path.to_string_lossy().to_string();
        if is_entry_path_dangerous(path) {
            return Err(GeeZipError::PathTraversal {
                entry: path_str.clone(),
                target: "WIM archive".into(),
            });
        }
        self.directories.push(path_str);
        Ok(())
    }

    fn finish(self: Box<Self>) -> GeeZipResult<u64> {
        let mut writer = self.writer.ok_or_else(|| GeeZipError::Format {
            message: "WIM writer not initialised (already consumed)".into(),
            format: ArchiveFormat::Wim,
        })?;

        // Build a sorted entry list: directories first, then files, all by path.
        let mut dir_entries: Vec<&str> = self.directories.iter().map(|s| s.as_str()).collect();
        dir_entries.sort();
        let mut file_entries: Vec<&PendingEntry> = self.entries.iter().collect();
        file_entries.sort_by(|a, b| a.path.cmp(&b.path));
        let mut spool = self.spool;

        // Ensure intermediate directories exist for each file.
        let mut all_dirs = std::collections::HashSet::new();
        for d in dir_entries.iter() {
            all_dirs.insert(d.to_string());
        }
        for file in &file_entries {
            let mut parent = std::path::Path::new(&file.path);
            while let Some(p) = parent.parent() {
                let p_str = p.to_string_lossy();
                if p_str.is_empty() || p_str == "." || p_str == "/" {
                    break;
                }
                all_dirs.insert(p_str.to_string());
                parent = p;
            }
        }

        // Merge sorted dir list with implicit parent dirs.
        let mut all_dirs_sorted: Vec<String> = all_dirs.into_iter().collect();
        all_dirs_sorted.sort();

        // Compute SHA-1 hashes for all file entries.
        let file_hashes: Vec<[u8; 20]> = file_entries.iter().map(|entry| entry.hash).collect();

        // Calculate offsets for file data (laid out sequentially after header).
        let mut current_offset: u64 = 208; // after header
        let mut file_resource_descriptors: Vec<ResourceDescriptor> = Vec::new();

        for entry in &file_entries {
            let rd = ResourceDescriptor {
                flags: 0, // not compressed, not metadata
                compressed_size: entry.size,
                offset: current_offset,
                original_size: entry.size,
            };
            current_offset = current_offset
                .checked_add(entry.size)
                .ok_or_else(|| GeeZipError::format("WIM data size overflow", ArchiveFormat::Wim))?;
            file_resource_descriptors.push(rd);
        }

        // ---- Build metadata resource ----
        let metadata_bytes = build_metadata(
            &all_dirs_sorted,
            &file_entries,
            &file_hashes,
            &file_resource_descriptors,
        )?;

        let metadata_rd = ResourceDescriptor {
            flags: 0x02, // metadata
            compressed_size: metadata_bytes.len() as u64,
            offset: current_offset,
            original_size: metadata_bytes.len() as u64,
        };
        current_offset += metadata_bytes.len() as u64;

        // ---- Build XML data resource ----
        let xml_bytes = build_xml(&all_dirs_sorted, &file_entries, &file_resource_descriptors);

        let xml_rd = ResourceDescriptor {
            flags: 0,
            compressed_size: xml_bytes.len() as u64,
            offset: current_offset,
            original_size: xml_bytes.len() as u64,
        };
        current_offset += xml_bytes.len() as u64;

        // ---- Build lookup table ----
        let lookup_bytes = build_lookup_table(
            &metadata_rd,
            &file_resource_descriptors,
            &file_hashes,
            super::sha1::compute_sha1(&metadata_bytes),
        );

        let offset_table_rd = ResourceDescriptor {
            flags: 0,
            compressed_size: lookup_bytes.len() as u64,
            offset: current_offset,
            original_size: lookup_bytes.len() as u64,
        };

        // ---- Build header ----
        let header = WimHeader {
            image_count: if all_dirs_sorted.is_empty() && file_entries.is_empty() {
                0
            } else {
                1
            },
            flags: 0, // no compression flag
            chunk_size: self.chunk_size,
            guid: self.guid,
            part_number: 1,
            total_parts: 1,
            offset_table: offset_table_rd,
            xml_data: xml_rd,
            boot_metadata: ResourceDescriptor {
                flags: 0,
                compressed_size: 0,
                offset: 0,
                original_size: 0,
            },
            boot_index: 0,
            integrity: ResourceDescriptor {
                flags: 0,
                compressed_size: 0,
                offset: 0,
                original_size: 0,
            },
        };

        let header_bytes = header.to_bytes();

        // ---- Write everything ----
        writer
            .write_all(&header_bytes)
            .map_err(|e| GeeZipError::io(e, "writing WIM header"))?;
        let mut total_written = header_bytes.len() as u64;

        // Write file data blocks
        for entry in &file_entries {
            let spool = spool.as_mut().ok_or_else(|| {
                GeeZipError::format("WIM data spool is missing", ArchiveFormat::Wim)
            })?;
            spool
                .seek(SeekFrom::Start(entry.offset))
                .map_err(|e| GeeZipError::io(e, "seeking WIM file data"))?;
            let copied = io::copy(&mut spool.take(entry.size), &mut writer)
                .map_err(|e| GeeZipError::io(e, "writing WIM file data"))?;
            if copied != entry.size {
                return Err(GeeZipError::io(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "truncated WIM data spool"),
                    "writing WIM file data",
                ));
            }
            total_written += copied;
        }

        // Write metadata
        writer
            .write_all(&metadata_bytes)
            .map_err(|e| GeeZipError::io(e, "writing WIM metadata"))?;
        total_written += metadata_bytes.len() as u64;

        // Write XML data
        writer
            .write_all(&xml_bytes)
            .map_err(|e| GeeZipError::io(e, "writing WIM XML data"))?;
        total_written += xml_bytes.len() as u64;

        // Write lookup table
        writer
            .write_all(&lookup_bytes)
            .map_err(|e| GeeZipError::io(e, "writing WIM lookup table"))?;
        total_written += lookup_bytes.len() as u64;

        writer
            .flush()
            .map_err(|e| GeeZipError::io(e, "flushing WIM file"))?;

        Ok(total_written)
    }
}

// ---------------------------------------------------------------------------
// Metadata resource builder
// ---------------------------------------------------------------------------

fn build_metadata(
    dirs: &[String],
    files: &[&PendingEntry],
    hashes: &[[u8; 20]],
    _file_rds: &[ResourceDescriptor],
) -> GeeZipResult<Vec<u8>> {
    let zero_hash = [0u8; 20];
    let zero_rd = ResourceDescriptor {
        flags: 0,
        compressed_size: 0,
        offset: 0,
        original_size: 0,
    };

    // Build tree.
    let mut root = TreeEntry {
        path: String::new(),
        name: String::new(),
        is_dir: true,
        hash: zero_hash,
        size: 0,
        rd: zero_rd,
        children: Vec::new(),
    };

    for d in dirs {
        insert_into_tree(&mut root, d, true, zero_hash, 0, zero_rd);
    }
    for (i, entry) in files.iter().enumerate() {
        insert_into_tree(
            &mut root,
            &entry.path,
            false,
            hashes[i],
            entry.size,
            _file_rds[i],
        );
    }

    // Phase 1: collect root-level entries (sorted: dirs first, then files)
    let mut children: Vec<&TreeEntry> = root.children.iter().collect();
    children.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).reverse().then(a.name.cmp(&b.name)));

    // A metadata resource starts with a nameless root dentry and its terminator.
    // The actual root children follow at the root's subdirectory offset.
    const SECURITY_BLOCK_LEN: u64 = 8;
    let mut buf = serialize_entry(&root)?;
    buf.extend_from_slice(&0u64.to_le_bytes());
    let mut offset = SECURITY_BLOCK_LEN + buf.len() as u64;
    if !children.is_empty() {
        buf[16..24].copy_from_slice(&offset.to_le_bytes());
    }
    let mut dir_pending: Vec<(u64, &TreeEntry)> = Vec::new();

    // Write entries for a level, collecting directories that need child processing.
    // Returns entries at this level.
    fn write_level<'a>(
        entries: &[&'a TreeEntry],
        buf: &mut Vec<u8>,
        offset: &mut u64,
        pending: &mut Vec<(u64, &'a TreeEntry)>,
    ) -> GeeZipResult<()> {
        for entry in entries {
            let entry_data = serialize_entry(entry)?;
            let padded = (entry_data.len() + 7) & !7;
            buf.extend_from_slice(&entry_data);
            buf.resize(buf.len() + (padded - entry_data.len()), 0);

            if entry.is_dir && !entry.children.is_empty() {
                let subdir_pos = (*offset - SECURITY_BLOCK_LEN + 16) as usize;
                pending.push((subdir_pos as u64, entry));
            }
            *offset += padded as u64;
        }
        Ok(())
    }

    write_level(&children, &mut buf, &mut offset, &mut dir_pending)?;

    // End marker for root entries
    buf.extend_from_slice(&0u64.to_le_bytes());
    offset += 8;

    // Phase 3: recursively write children for each directory at their own positions.
    // Process pending dirs in order, which may add new pending dirs.
    let mut i = 0;
    while i < dir_pending.len() {
        let (subdir_pos, dir_entry) = dir_pending[i];
        let child_start = offset;

        let mut sub_children: Vec<&TreeEntry> = dir_entry.children.iter().collect();
        sub_children.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).reverse().then(a.name.cmp(&b.name)));

        write_level(&sub_children, &mut buf, &mut offset, &mut dir_pending)?;

        // End marker for this directory's children
        buf.extend_from_slice(&0u64.to_le_bytes());
        offset += 8;

        // Patch subdir_offset in the parent entry
        let pos = subdir_pos as usize;
        if pos + 8 <= buf.len() {
            buf[pos..pos + 8].copy_from_slice(&child_start.to_le_bytes());
        }

        i += 1;
    }

    // Prepend security block
    let mut result = Vec::with_capacity(8 + buf.len());
    result.extend_from_slice(&8u32.to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&buf);

    Ok(result)
}

fn insert_into_tree(
    root: &mut TreeEntry,
    full_path: &str,
    is_dir: bool,
    hash: [u8; 20],
    size: u64,
    rd: ResourceDescriptor,
) {
    let path = std::path::Path::new(full_path);
    let components: Vec<String> = path
        .components()
        .filter_map(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.is_empty() || s == "." || s == "/" {
                None
            } else {
                Some(s.into_owned())
            }
        })
        .collect();

    if components.is_empty() {
        return;
    }

    let mut current = root;
    let last_idx = components.len() - 1;

    for (i, comp) in components.iter().enumerate() {
        let is_last = i == last_idx;
        // Find or create child
        let child_idx = current.children.iter().position(|c| c.name == *comp);
        if is_last {
            // This is the target entry
            if let Some(idx) = child_idx {
                // Update existing (file replaces directory placeholder)
                current.children[idx].is_dir = is_dir;
                current.children[idx].hash = hash;
                current.children[idx].size = size;
                current.children[idx].rd = rd;
            } else {
                // Build partial path for this entry
                let partial_path = components[..=i].join("/");
                current.children.push(TreeEntry {
                    path: partial_path,
                    name: comp.to_string(),
                    is_dir,
                    hash,
                    size,
                    rd,
                    children: Vec::new(),
                });
            }
        } else {
            let entry = if let Some(idx) = child_idx {
                &mut current.children[idx]
            } else {
                let partial_path = components[..=i].join("/");
                current.children.push(TreeEntry {
                    path: partial_path,
                    name: comp.to_string(),
                    is_dir: true,
                    hash: [0u8; 20],
                    size: 0,
                    rd: ResourceDescriptor {
                        flags: 0,
                        compressed_size: 0,
                        offset: 0,
                        original_size: 0,
                    },
                    children: Vec::new(),
                });
                current.children.last_mut().unwrap()
            };
            current = entry;
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct TreeEntry {
    path: String,
    name: String,
    is_dir: bool,
    hash: [u8; 20],
    size: u64,
    rd: ResourceDescriptor,
    children: Vec<TreeEntry>,
}

// ---------------------------------------------------------------------------
// Entry serialization
// ---------------------------------------------------------------------------

/// Serialize a single directory entry (variable length, 8-byte aligned).
fn serialize_entry(entry: &TreeEntry) -> GeeZipResult<Vec<u8>> {
    let name_utf16: Vec<u8> = encode_utf16le(&entry.name);
    let file_name_len = name_utf16.len();
    u16::try_from(file_name_len).map_err(|_| {
        GeeZipError::format(
            "WIM filename exceeds UTF-16 length limit",
            ArchiveFormat::Wim,
        )
    })?;

    // Length includes the 102-byte fixed dentry, optional terminated name and padding.
    let name_size = if file_name_len == 0 {
        0
    } else {
        file_name_len + 2
    };
    let padded_length = (102 + name_size + 7) & !7;
    let mut buf = Vec::with_capacity(padded_length);
    buf.extend_from_slice(&(padded_length as u64).to_le_bytes());

    // 98 bytes fixed fields
    let attributes: u32 = if entry.is_dir {
        0x0000_0010
    } else {
        0x0000_0020
    };
    buf.extend_from_slice(&attributes.to_le_bytes()); // offset 0: attributes

    // No security descriptors are stored in this image.
    buf.extend_from_slice(&u32::MAX.to_le_bytes());

    // subdir_offset placeholder (8 bytes) — patched later by caller
    buf.extend_from_slice(&0u64.to_le_bytes()); // offset 8

    // unused_1 (8 bytes)
    buf.extend_from_slice(&0u64.to_le_bytes()); // offset 16

    // unused_2 (8 bytes)
    buf.extend_from_slice(&0u64.to_le_bytes()); // offset 24

    // creation_time, last_access_time, last_write_time (FILETIME)
    let now_ft = windows_filetime_now();
    buf.extend_from_slice(&now_ft.to_le_bytes()); // offset 32: creation_time
    buf.extend_from_slice(&now_ft.to_le_bytes()); // offset 40: last_access_time
    buf.extend_from_slice(&now_ft.to_le_bytes()); // offset 48: last_write_time

    // hash (20 bytes)
    buf.extend_from_slice(&entry.hash); // offset 56

    // Unknown field plus hard-link/reparse union: 12 bytes in the on-disk format.
    buf.extend_from_slice(&[0u8; 12]);

    // stream_count (2 bytes)
    buf.extend_from_slice(&0u16.to_le_bytes()); // offset 92

    // short_name_len (2 bytes) — 0, no short name
    buf.extend_from_slice(&0u16.to_le_bytes()); // offset 94

    // file_name_len (2 bytes)
    buf.extend_from_slice(&(file_name_len as u16).to_le_bytes()); // offset 96

    if file_name_len != 0 {
        buf.extend_from_slice(&name_utf16);
        buf.extend_from_slice(&0u16.to_le_bytes());
    }
    buf.resize(padded_length, 0);

    Ok(buf)
}

// ---------------------------------------------------------------------------
// XML data builder
// ---------------------------------------------------------------------------

fn build_xml(
    dirs: &[String],
    files: &[&PendingEntry],
    _file_rds: &[ResourceDescriptor],
) -> Vec<u8> {
    let dir_count = dirs.len() as u64;
    let file_count = files.len() as u64;
    let total_bytes: u64 = files.iter().map(|entry| entry.size).sum();

    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<WIM>
  <IMAGE INDEX="1">
    <NAME>GeeZipX Image</NAME>
    <DIRCOUNT>{dir_count}</DIRCOUNT>
    <FILECOUNT>{file_count}</FILECOUNT>
    <TOTALBYTES>{total_bytes}</TOTALBYTES>
    <CREATIONTIME>
      <HIGHPART>0x00000000</HIGHPART>
      <LOWPART>0x00000000</LOWPART>
    </CREATIONTIME>
    <LASTMODIFICATIONTIME>
      <HIGHPART>0x00000000</HIGHPART>
      <LOWPART>0x00000000</LOWPART>
    </LASTMODIFICATIONTIME>
  </IMAGE>
</WIM>
"#,
    );

    // Encode as UTF-16LE with BOM
    let mut out = Vec::new();
    // BOM: 0xFF 0xFE
    out.push(0xFF);
    out.push(0xFE);
    // UTF-16LE encoded XML
    for ch in xml.encode_utf16() {
        out.extend_from_slice(&ch.to_le_bytes());
    }

    out
}

// ---------------------------------------------------------------------------
// Lookup table builder
// ---------------------------------------------------------------------------

fn build_lookup_table(
    metadata_rd: &ResourceDescriptor,
    file_rds: &[ResourceDescriptor],
    file_hashes: &[[u8; 20]],
    metadata_hash: [u8; 20],
) -> Vec<u8> {
    let num_entries = 1 + file_rds.len(); // metadata + files
    let mut buf = Vec::with_capacity(num_entries * 50);

    // Metadata resource entry
    buf.extend_from_slice(&metadata_rd.to_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // part_number (1-based)
    buf.extend_from_slice(&1u32.to_le_bytes()); // ref_count
    buf.extend_from_slice(&metadata_hash);

    // File resource entries
    for (i, rd) in file_rds.iter().enumerate() {
        buf.extend_from_slice(&rd.to_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // part_number (1-based)
        buf.extend_from_slice(&1u32.to_le_bytes()); // ref_count
        buf.extend_from_slice(&file_hashes[i]);
    }

    buf
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a Rust string to UTF-16LE bytes (no BOM).
fn encode_utf16le(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for ch in s.encode_utf16() {
        out.extend_from_slice(&ch.to_le_bytes());
    }
    out
}

/// Get the current time as a Windows FILETIME value (100-ns intervals since 1601-01-01).
fn windows_filetime_now() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let intervals = since_epoch.as_secs() * 10_000_000 + since_epoch.subsec_nanos() as u64 / 100;
    intervals + 11_644_473_600_000_000_u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spools_entry_bytes_and_recovers_after_failed_input() {
        struct FailsAfterPrefix(bool);
        impl Read for FailsAfterPrefix {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::Error::other("input failed"));
                }
                self.0 = true;
                buffer[..4].copy_from_slice(b"junk");
                Ok(4)
            }
        }
        let mut writer = WimWriter::new(Vec::new());
        assert!(writer
            .add_entry_from_reader(Path::new("failed"), &mut FailsAfterPrefix(false))
            .is_err());
        assert!(writer.entries.is_empty());
        assert_eq!(writer.spool_len, 0);
        let length = 8 * 1024 * 1024;
        writer
            .add_entry_from_reader(Path::new("large.bin"), &mut io::repeat(0x5a).take(length))
            .unwrap();
        assert_eq!(writer.entries[0].size, length);
        assert_eq!(
            writer.spool.as_ref().unwrap().metadata().unwrap().len(),
            length
        );
        let expected_hash: [u8; 20] = Sha1::digest(vec![0x5a; length as usize]).into();
        assert_eq!(writer.entries[0].hash, expected_hash);
        assert!(Box::new(writer).finish().unwrap() > length);
    }

    #[test]
    fn sorted_files_keep_correct_spooled_contents() {
        use crate::archive::ArchiveReader;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("sorted.wim");
        let mut writer = WimWriter::new(File::create(&path).unwrap());
        writer
            .add_entry_from_reader(Path::new("z.txt"), &mut &b"last"[..])
            .unwrap();
        writer
            .add_entry_from_reader(Path::new("a.txt"), &mut &b"first"[..])
            .unwrap();
        Box::new(writer).finish().unwrap();
        let mut reader = super::super::WimReader::open(&path).unwrap();
        let entries = reader.entries().unwrap();
        for (entry, expected) in [("a.txt", &b"first"[..]), ("z.txt", &b"last"[..])] {
            let mut output = Vec::new();
            reader
                .extract(
                    entries.iter().find(|item| item.path == entry).unwrap(),
                    &mut output,
                )
                .unwrap();
            assert_eq!(output, expected);
        }
    }

    #[test]
    fn rejects_names_that_cannot_fit_in_wim_metadata() {
        let mut writer = WimWriter::new(Vec::new());
        writer.add_directory(Path::new(&"x".repeat(32768))).unwrap();
        assert!(Box::new(writer).finish().is_err());
    }

    #[test]
    fn wim_writer_empty() {
        let writer = WimWriter::new(Vec::new());
        let result = Box::new(writer).finish().unwrap();
        // Empty WIM should still have a valid header + empty metadata + XML + lookup
        assert!(result > 208);
    }

    #[test]
    fn wim_writer_single_file() {
        let mut writer = WimWriter::new(Vec::new());
        writer
            .add_entry_from_reader(
                Path::new("hello.txt"),
                &mut std::io::Cursor::new(b"hello wim\n"),
            )
            .unwrap();
        let bytes_written = Box::new(writer).finish().unwrap();
        assert!(bytes_written > 208);
    }

    #[test]
    fn wim_writer_with_dirs() {
        let mut writer = WimWriter::new(Vec::new());
        writer.add_directory(Path::new("subdir")).unwrap();
        writer
            .add_entry_from_reader(
                Path::new("subdir/nested.txt"),
                &mut std::io::Cursor::new(b"nested content"),
            )
            .unwrap();
        let bytes_written = Box::new(writer).finish().unwrap();
        assert!(bytes_written > 208);
    }

    #[test]
    fn wim_writer_path_traversal_rejected() {
        let buf = Vec::new();
        let mut writer = WimWriter::new(buf);
        let err = writer
            .add_entry_from_reader(
                Path::new("../escape.txt"),
                &mut std::io::Cursor::new(b"bad"),
            )
            .unwrap_err();
        assert!(matches!(err, GeeZipError::PathTraversal { .. }));
    }

    #[test]
    fn wim_writer_roundtrip_small() {
        use crate::archive::ArchiveReader;
        use std::io::Cursor;

        let tmp = tempfile::tempdir().unwrap();
        let wim_path = tmp.path().join("test.wim");
        let file = std::fs::File::create(&wim_path).unwrap();
        let mut writer = WimWriter::new(file);
        writer.add_directory(Path::new("mydir")).unwrap();
        writer
            .add_entry_from_reader(
                Path::new("mydir/readme.txt"),
                &mut Cursor::new(b"Hello from GeeZipX WIM writer!\n"),
            )
            .unwrap();
        writer
            .add_entry_from_reader(
                Path::new("root_file.bin"),
                &mut Cursor::new(b"binary\x00\x01\x02\x03"),
            )
            .unwrap();

        Box::new(writer).finish().unwrap();

        let mut reader = super::super::WimReader::open(&wim_path).unwrap();
        let entries = reader.entries().unwrap();

        assert_eq!(entries.len(), 3, "expected 3 entries, got: {entries:?}");

        let dir_entry = entries.iter().find(|e| e.path == "mydir").unwrap();
        assert!(dir_entry.is_dir);

        let readme = entries
            .iter()
            .find(|e| e.path == "mydir/readme.txt")
            .unwrap();
        assert!(!readme.is_dir);
        assert_eq!(readme.size, 31);

        let root_file = entries.iter().find(|e| e.path == "root_file.bin").unwrap();
        assert!(!root_file.is_dir);
        assert_eq!(root_file.size, 10);
    }

    #[test]
    fn wim_writer_empty_dirs() {
        use crate::archive::ArchiveReader;
        use std::io::Cursor;

        let tmp = tempfile::tempdir().unwrap();
        let wim_path = tmp.path().join("emptydirs.wim");
        let file = std::fs::File::create(&wim_path).unwrap();
        let mut writer = WimWriter::new(file);
        writer.add_directory(Path::new("empty_dir")).unwrap();
        writer.add_directory(Path::new("a")).unwrap();
        writer.add_directory(Path::new("a/b")).unwrap();
        writer
            .add_entry_from_reader(Path::new("a/b/c.txt"), &mut Cursor::new(b"deep file"))
            .unwrap();

        Box::new(writer).finish().unwrap();

        let mut reader = super::super::WimReader::open(&wim_path).unwrap();
        let entries = reader.entries().unwrap();

        assert!(
            entries.len() == 4,
            "expected 4 entries, got {}: {:?}",
            entries.len(),
            entries
        );

        let empty_dir = entries.iter().find(|e| e.path == "empty_dir").unwrap();
        assert!(empty_dir.is_dir);

        let deep_file = entries.iter().find(|e| e.path == "a/b/c.txt").unwrap();
        assert!(!deep_file.is_dir);
    }
}
