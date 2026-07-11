//! WIM writer — creates uncompressed WIM archives.
//!
//! All file data is stored raw (CompressionType::None). The generated
//! archives are fully valid per the WIM spec and can be read by any
//! compliant tool (wimlib, 7-Zip, etc.).

use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::archive::{is_entry_path_dangerous, ArchiveWriter};
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::{ResourceDescriptor, WimHeader};
use super::sha1::compute_sha1;

/// Writer for creating uncompressed WIM (.wim) archives.
pub struct WimWriter<W: Write + Send> {
    writer: Option<W>,
    format: ArchiveFormat,
    entries: Vec<(String, Vec<u8>)>,
    directories: Vec<String>,
    chunk_size: u32,
    guid: [u8; 16],
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
            directories: Vec::new(),
            chunk_size: 32768,
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
        let mut data = Vec::new();
        reader
            .read_to_end(&mut data)
            .map_err(|e| GeeZipError::io(e, format!("reading entry '{path_str}'")))?;
        self.entries.push((path_str, data));
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
        let mut file_entries: Vec<&(String, Vec<u8>)> = self.entries.iter().collect();
        file_entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Ensure intermediate directories exist for each file.
        let mut all_dirs = std::collections::HashSet::new();
        for d in dir_entries.iter() {
            all_dirs.insert(d.to_string());
        }
        for (path, _) in &file_entries {
            let mut parent = std::path::Path::new(path);
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
        let file_hashes: Vec<[u8; 20]> = file_entries
            .iter()
            .map(|(_, data)| compute_sha1(data))
            .collect();

        // Calculate offsets for file data (laid out sequentially after header).
        let mut current_offset: u64 = 208; // after header
        let mut file_resource_descriptors: Vec<ResourceDescriptor> = Vec::new();

        for (_, data) in &file_entries {
            let rd = ResourceDescriptor {
                flags: 0, // not compressed, not metadata
                compressed_size: data.len() as u64,
                offset: current_offset,
                original_size: data.len() as u64,
            };
            current_offset += data.len() as u64;
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
        let lookup_bytes =
            build_lookup_table(&metadata_rd, &file_resource_descriptors, &file_hashes);

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
        for (_, data) in &file_entries {
            writer
                .write_all(data)
                .map_err(|e| GeeZipError::io(e, "writing WIM file data"))?;
            total_written += data.len() as u64;
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
    files: &[&(String, Vec<u8>)],
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
    for (i, (path, _data)) in files.iter().enumerate() {
        insert_into_tree(
            &mut root,
            path,
            false,
            hashes[i],
            files[i].1.len() as u64,
            _file_rds[i],
        );
    }

    // Phase 1: collect root-level entries (sorted: dirs first, then files)
    let mut children: Vec<&TreeEntry> = root.children.iter().collect();
    children.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).reverse().then(a.name.cmp(&b.name)));

    // Phase 2: write root entries at position 8 (after security block)
    const SECURITY_BLOCK_LEN: u64 = 8;
    let mut buf = Vec::new();
    let mut offset: u64 = SECURITY_BLOCK_LEN;
    let mut dir_pending: Vec<(u64, &TreeEntry)> = Vec::new();

    // Write entries for a level, collecting directories that need child processing.
    // Returns entries at this level.
    fn write_level<'a>(
        entries: &[&'a TreeEntry],
        buf: &mut Vec<u8>,
        offset: &mut u64,
        pending: &mut Vec<(u64, &'a TreeEntry)>,
    ) {
        for entry in entries {
            let entry_data = serialize_entry(entry).unwrap();
            let padded = (entry_data.len() + 7) & !7;
            buf.extend_from_slice(&entry_data);
            buf.resize(buf.len() + (padded - entry_data.len()), 0);

            if entry.is_dir && !entry.children.is_empty() {
                let subdir_pos = (*offset - SECURITY_BLOCK_LEN + 16) as usize;
                pending.push((subdir_pos as u64, entry));
            }
            *offset += padded as u64;
        }
    }

    write_level(&children, &mut buf, &mut offset, &mut dir_pending);

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

        write_level(&sub_children, &mut buf, &mut offset, &mut dir_pending);

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
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.is_empty() || s == "." || s == "/" {
                None
            } else {
                Some(Box::leak(s.into_owned().into_boxed_str()) as &str)
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

    // Content length = 98 (fixed) + file_name_len (before padding)
    let content_len = 98 + file_name_len;
    // Padded content length (aligned to 8 bytes)
    let padded_content = (content_len + 7) & !7;

    let mut buf = Vec::with_capacity(8 + padded_content);

    // 8-byte length prefix = unpadded content length (the reader does its own alignment)
    buf.extend_from_slice(&(content_len as u64).to_le_bytes());

    // 98 bytes fixed fields
    let attributes: u32 = if entry.is_dir {
        0x0000_0010
    } else {
        0x0000_0020
    };
    buf.extend_from_slice(&attributes.to_le_bytes()); // offset 0: attributes

    // security_id (4 bytes) — always 0
    buf.extend_from_slice(&0u32.to_le_bytes()); // offset 4: security_id

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

    // reserved (16 bytes, zero-filled — bytes 76-91)
    buf.extend_from_slice(&[0u8; 16]); // offset 76

    // stream_count (2 bytes)
    buf.extend_from_slice(&0u16.to_le_bytes()); // offset 92

    // short_name_len (2 bytes) — 0, no short name
    buf.extend_from_slice(&0u16.to_le_bytes()); // offset 94

    // file_name_len (2 bytes)
    buf.extend_from_slice(&(file_name_len as u16).to_le_bytes()); // offset 96

    // file name (variable, UTF-16LE)
    buf.extend_from_slice(&name_utf16);

    // Pad to 8-byte boundary (after the 8-byte prefix)
    let current_content_len = buf.len() - 8; // length of content written so far
    let pad_needed = padded_content - current_content_len;
    buf.extend(std::iter::repeat_n(0u8, pad_needed));

    Ok(buf)
}

// ---------------------------------------------------------------------------
// XML data builder
// ---------------------------------------------------------------------------

fn build_xml(
    dirs: &[String],
    files: &[&(String, Vec<u8>)],
    _file_rds: &[ResourceDescriptor],
) -> Vec<u8> {
    let dir_count = dirs.len() as u64;
    let file_count = files.len() as u64;
    let total_bytes: u64 = files.iter().map(|(_, d)| d.len() as u64).sum();

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
) -> Vec<u8> {
    let num_entries = 1 + file_rds.len(); // metadata + files
    let mut buf = Vec::with_capacity(num_entries * 50);

    // Metadata resource entry
    buf.extend_from_slice(&metadata_rd.to_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // part_number
    buf.extend_from_slice(&1u32.to_le_bytes()); // ref_count
    buf.extend_from_slice(&[0u8; 20]); // SHA-1 hash (all zeros for metadata)

    // File resource entries
    for (i, rd) in file_rds.iter().enumerate() {
        buf.extend_from_slice(&rd.to_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // part_number
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
