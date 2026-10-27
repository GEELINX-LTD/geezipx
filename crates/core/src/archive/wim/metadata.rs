//! WIM metadata resource parsing — directory entry tree walker.
//!
//! The metadata resource contains a security block followed by a
//! depth-first directory entry tree.  We walk the tree recursively
//! and produce a flat `Vec<Entry>` with full paths.

use std::collections::{HashMap, HashSet};

use crate::archive::Entry;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::{ResourceDescriptor, WimHeader};
use super::lookup::{LookupTable, Sha1Hash};
use super::resource;

/// FILE_ATTRIBUTE_DIRECTORY constant.
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
/// Bound stack use and path growth for untrusted directory trees.
const MAX_DIRECTORY_DEPTH: usize = 256;

struct WalkState {
    visited: HashSet<u64>,
    entries_start: u64,
    legacy_layout: bool,
}

// ---------------------------------------------------------------------------
// Internal representation of a directory entry during tree walk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DirEntry {
    /// Full forward-slash-separated path within the image.
    path: String,
    /// Uncompressed size (0 for directories, or from streams).
    size: u64,
    /// Whether this entry is a directory.
    is_dir: bool,
    /// SHA-1 hash of the file data (all zeros = no data).
    hash: Sha1Hash,
    /// Last modification time as Unix timestamp.
    modified: Option<u64>,
    /// Offset of the subdirectory (0 = leaf, non-zero = child directory).
    subdir_offset: u64,
    /// Number of alternate data streams.
    #[allow(dead_code)]
    stream_count: u16,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk the metadata resource for `image_index` and collect a flat entry list
/// together with a path → resource-descriptor map for extraction.
pub(crate) fn walk_directory_tree(
    file: &mut std::fs::File,
    image_index: usize,
    header: &WimHeader,
    lookup: &LookupTable,
) -> GeeZipResult<(Vec<Entry>, HashMap<String, ResourceDescriptor>)> {
    if image_index >= lookup.metadata_streams.len() {
        return Err(GeeZipError::format(
            format!(
                "image index {} out of range ({} images)",
                image_index + 1,
                lookup.metadata_streams.len()
            ),
            ArchiveFormat::Wim,
        ));
    }

    let meta_desc = &lookup.metadata_streams[image_index].resource;
    let meta_bytes = resource::read_resource(
        file,
        meta_desc,
        header.compression_type(),
        header.chunk_size,
    )?;

    let meta_stream = &lookup.metadata_streams[image_index];
    if !is_zero_hash(&meta_stream.hash)
        && super::sha1::compute_sha1(&meta_bytes) != meta_stream.hash.0
    {
        return Err(invalid_metadata("WIM metadata SHA-1 mismatch"));
    }

    // Build a hash → resource map from the lookup table for fast extraction.
    let mut hash_to_resource: HashMap<Sha1Hash, ResourceDescriptor> = HashMap::new();
    for sd in &lookup.file_streams {
        hash_to_resource.insert(sd.hash.clone(), sd.resource);
    }

    // Old GeeZipX writers used an invalid nonzero chunk size with no compression,
    // and a different dentry layout. Retain read compatibility for those archives.
    if header.chunk_size != 0 && header.compression_type() == super::header::CompressionType::None {
        parse_directory_tree_with_layout(&meta_bytes, &hash_to_resource, true)
    } else {
        parse_directory_tree(&meta_bytes, &hash_to_resource)
    }
}

fn parse_directory_tree(
    meta_bytes: &[u8],
    hash_to_resource: &HashMap<Sha1Hash, ResourceDescriptor>,
) -> GeeZipResult<(Vec<Entry>, HashMap<String, ResourceDescriptor>)> {
    parse_directory_tree_with_layout(meta_bytes, hash_to_resource, false)
}

fn parse_directory_tree_with_layout(
    meta_bytes: &[u8],
    hash_to_resource: &HashMap<Sha1Hash, ResourceDescriptor>,
    legacy_layout: bool,
) -> GeeZipResult<(Vec<Entry>, HashMap<String, ResourceDescriptor>)> {
    // ---- Parse security block ----
    if meta_bytes.len() < 8 {
        return Err(invalid_metadata("metadata too short for security block"));
    }
    let sec_total_len = read_u32_le(meta_bytes, 0) as usize;
    let sec_num_entries = read_u32_le(meta_bytes, 4) as usize;
    let sizes_end = sec_num_entries
        .checked_mul(8)
        .and_then(|len| len.checked_add(8))
        .ok_or_else(|| invalid_metadata("security descriptor sizes overflow"))?;
    if sec_total_len < sizes_end || sec_total_len > meta_bytes.len() {
        return Err(invalid_metadata("invalid security block length"));
    }
    let mut descriptors_end = sizes_end;
    for offset in (8..sizes_end).step_by(8) {
        let size = usize::try_from(read_u64_le(meta_bytes, offset))
            .map_err(|_| invalid_metadata("security descriptor size overflow"))?;
        descriptors_end = bounded_end(descriptors_end, size, sec_total_len)?;
    }
    let cursor = align_eight(sec_total_len)?;

    // ---- Walk root directory entries ----
    let root_entries = read_directory_entries_with_layout(meta_bytes, cursor, legacy_layout)?;
    let mut walk = WalkState {
        visited: HashSet::from([cursor as u64]),
        entries_start: cursor as u64,
        legacy_layout,
    };
    let mut entries: Vec<Entry> = Vec::new();
    let mut path_to_resource: HashMap<String, ResourceDescriptor> = HashMap::new();

    for de in root_entries {
        collect_entries(
            meta_bytes,
            &de,
            hash_to_resource,
            &mut entries,
            &mut path_to_resource,
            &mut walk,
            0,
        )?;
    }

    Ok((entries, path_to_resource))
}

// ---------------------------------------------------------------------------
// Recursive entry collection
// ---------------------------------------------------------------------------

fn collect_entries(
    meta_bytes: &[u8],
    entry: &DirEntry,
    hash_to_resource: &HashMap<Sha1Hash, ResourceDescriptor>,
    entries: &mut Vec<Entry>,
    path_to_resource: &mut HashMap<String, ResourceDescriptor>,
    walk: &mut WalkState,
    depth: usize,
) -> GeeZipResult<()> {
    // Determine size from the primary stream hash
    let size = if entry.is_dir {
        0
    } else {
        // Look up the hash to find the resource (for size info)
        if is_zero_hash(&entry.hash) {
            entry.size
        } else if let Some(rd) = hash_to_resource.get(&entry.hash) {
            path_to_resource.insert(entry.path.clone(), *rd);
            rd.original_size
        } else {
            return Err(invalid_metadata(format!(
                "missing resource for entry '{}'",
                entry.path
            )));
        }
    };

    let archive_entry = Entry {
        path: entry.path.clone(),
        size,
        compressed_size: 0,
        crc32: None,
        modified: entry.modified,
        is_dir: entry.is_dir,
    };
    if !entry.path.is_empty() {
        entries.push(archive_entry);
    }

    // Recurse into subdirectory if present
    if entry.subdir_offset != 0 {
        if !entry.is_dir {
            return Err(invalid_metadata("file entry has a subdirectory offset"));
        }
        if depth >= MAX_DIRECTORY_DEPTH {
            return Err(invalid_metadata("directory tree exceeds depth limit"));
        }
        if entry.subdir_offset < walk.entries_start || !entry.subdir_offset.is_multiple_of(8) {
            return Err(invalid_metadata("invalid subdirectory offset"));
        }
        if !walk.visited.insert(entry.subdir_offset) {
            return Err(invalid_metadata(
                "repeated subdirectory offset (cycle or alias)",
            ));
        }
        let offset = usize::try_from(entry.subdir_offset)
            .map_err(|_| invalid_metadata("subdirectory offset overflow"))?;
        let sub_entries =
            read_directory_entries_with_layout(meta_bytes, offset, walk.legacy_layout)?;
        for sub_de in sub_entries {
            // Build full path
            let full_path = if entry.path.is_empty() {
                sub_de.path.clone()
            } else {
                format!("{}/{}", entry.path, sub_de.path)
            };
            let sub_de_full = DirEntry {
                path: full_path,
                ..sub_de
            };
            collect_entries(
                meta_bytes,
                &sub_de_full,
                hash_to_resource,
                entries,
                path_to_resource,
                walk,
                depth + 1,
            )?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Low-level entry parsing
// ---------------------------------------------------------------------------

/// Read directory entries at a specific absolute offset. Returns entries.
#[cfg(test)]
fn read_directory_entries_at(data: &[u8], offset: usize) -> GeeZipResult<Vec<DirEntry>> {
    read_directory_entries_with_layout(data, offset, false)
}

fn read_directory_entries_with_layout(
    data: &[u8],
    mut offset: usize,
    legacy: bool,
) -> GeeZipResult<Vec<DirEntry>> {
    let mut entries = Vec::new();

    loop {
        let entry_start = bounded_end(offset, 8, data.len())?;
        let entry_len = usize::try_from(read_u64_le(data, offset))
            .map_err(|_| invalid_metadata("directory entry length overflow"))?;
        if entry_len == 0 {
            // A complete end-of-entries marker is mandatory.
            break;
        }
        let entry_end = bounded_end(
            if legacy { entry_start } else { offset },
            align_eight(entry_len)?,
            data.len(),
        )?;
        let entry_data = &data[entry_start..entry_end];
        let fixed = if legacy { 98 } else { 94 };

        if entry_data.len() < fixed {
            return Err(invalid_metadata(
                "directory entry too short for fixed fields",
            ));
        }

        // Parse fixed fields (98 bytes)
        let attributes = read_u32_le(entry_data, 0);
        let subdir_offset = read_u64_le(entry_data, 8);
        let creation_time = read_u64_le(entry_data, 32);
        let last_access_time = read_u64_le(entry_data, 40);
        let last_write_time = read_u64_le(entry_data, 48);

        let mut hash = [0u8; 20];
        hash.copy_from_slice(&entry_data[56..76]);

        let stream_count = read_u16_le(entry_data, fixed - 6);
        let short_name_len = read_u16_le(entry_data, fixed - 4) as usize;
        let file_name_len = read_u16_le(entry_data, fixed - 2) as usize;
        if !file_name_len.is_multiple_of(2) || !short_name_len.is_multiple_of(2) {
            return Err(invalid_metadata("odd UTF-16 name length"));
        }
        let file_name_end = bounded_end(fixed, file_name_len, entry_data.len())?;
        let names_end = bounded_end(file_name_end, short_name_len, entry_data.len())?;
        let terminators = if legacy {
            0
        } else {
            usize::from(file_name_len != 0) * 2 + usize::from(short_name_len != 0) * 2
        };
        bounded_end(names_end, terminators, entry_data.len())?;
        let file_name = decode_utf16le(&entry_data[fixed..file_name_end]);
        let mut var_offset = if legacy {
            align_eight(names_end)?
        } else {
            entry_end
        };

        // Modern extra streams follow the dentry, outside its length field.
        for _ in 0..stream_count {
            if legacy {
                let stream_start = bounded_end(var_offset, 8, entry_data.len())?;
                let stream_len = usize::try_from(read_u64_le(entry_data, var_offset))
                    .map_err(|_| invalid_metadata("alternate stream length overflow"))?;
                var_offset = align_eight(bounded_end(stream_start, stream_len, entry_data.len())?)?;
            } else {
                bounded_end(var_offset, 38, data.len())?;
                let stream_len = usize::try_from(read_u64_le(data, var_offset))
                    .map_err(|_| invalid_metadata("alternate stream length overflow"))?;
                let name_len = read_u16_le(data, var_offset + 36) as usize;
                let minimum = 38 + name_len + usize::from(name_len != 0) * 2;
                if stream_len < minimum || !name_len.is_multiple_of(2) {
                    return Err(invalid_metadata("invalid alternate stream length"));
                }
                var_offset = bounded_end(var_offset, align_eight(stream_len)?, data.len())?;
            }
        }

        let is_dir = (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0;
        let modified = filetime_to_option(last_write_time)
            .or_else(|| filetime_to_option(last_access_time))
            .or_else(|| filetime_to_option(creation_time));

        entries.push(DirEntry {
            path: file_name,
            size: 0, // determined later from lookup
            is_dir,
            hash: Sha1Hash(hash),
            modified,
            subdir_offset,
            stream_count,
        });

        offset = if legacy { entry_end } else { var_offset };
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn invalid_metadata(message: impl Into<String>) -> GeeZipError {
    GeeZipError::format(message, ArchiveFormat::Wim)
}

fn bounded_end(start: usize, len: usize, limit: usize) -> GeeZipResult<usize> {
    start
        .checked_add(len)
        .filter(|&end| end <= limit)
        .ok_or_else(|| invalid_metadata("truncated or overflowing metadata extent"))
}

fn align_eight(offset: usize) -> GeeZipResult<usize> {
    offset
        .checked_add(7)
        .map(|end| end & !7)
        .ok_or_else(|| invalid_metadata("metadata alignment overflow"))
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn decode_utf16le(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let u16s: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u16s)
}

fn is_zero_hash(hash: &Sha1Hash) -> bool {
    hash.0.iter().all(|&b| b == 0)
}

fn filetime_to_option(ft: u64) -> Option<u64> {
    const EPOCH_DIFF_100NS: u64 = 116444736000000000;
    ft.checked_sub(EPOCH_DIFF_100NS)
        .filter(|&unix_100ns| unix_100ns > 0)
        .map(|unix_100ns| unix_100ns / 10_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool, subdir_offset: u64) -> Vec<u8> {
        let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let len = (102 + name.len() + usize::from(!name.is_empty()) * 2 + 7) & !7;
        let mut bytes = vec![0; len];
        bytes[..8].copy_from_slice(&(len as u64).to_le_bytes());
        if is_dir {
            bytes[8..12].copy_from_slice(&FILE_ATTRIBUTE_DIRECTORY.to_le_bytes());
        }
        bytes[16..24].copy_from_slice(&subdir_offset.to_le_bytes());
        bytes[100..102].copy_from_slice(&(name.len() as u16).to_le_bytes());
        bytes[102..102 + name.len()].copy_from_slice(&name);
        bytes
    }

    fn metadata(root: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::from(8u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(root);
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes
    }

    fn parse(bytes: &[u8]) -> GeeZipResult<Vec<Entry>> {
        parse_directory_tree(bytes, &HashMap::new()).map(|(entries, _)| entries)
    }

    #[test]
    fn test_valid_nested_metadata_and_empty_directory() {
        let root = entry("目录", true, 0);
        let child_offset = 8 + root.len() + 8;
        let mut bytes = metadata(&root);
        bytes[24..32].copy_from_slice(&(child_offset as u64).to_le_bytes());
        bytes.extend_from_slice(&entry("文件.txt", false, 0));
        bytes.extend_from_slice(&0u64.to_le_bytes());
        let entries = parse(&bytes).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "目录");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].path, "目录/文件.txt");
        assert!(!entries[1].is_dir);
        assert!(parse(&metadata(&[])).unwrap().is_empty());
    }

    #[test]
    fn test_rejects_direct_indirect_cycles_and_shared_offsets() {
        let direct = metadata(&entry("loop", true, 8));
        assert!(parse(&direct)
            .unwrap_err()
            .to_string()
            .contains("repeated subdirectory"));

        let first = entry("parent", true, 0);
        let child_offset = (8 + first.len() + 8) as u64;
        let mut indirect = metadata(&entry("parent", true, child_offset));
        indirect.extend_from_slice(&entry("child", true, 8));
        indirect.extend_from_slice(&0u64.to_le_bytes());
        assert!(parse(&indirect)
            .unwrap_err()
            .to_string()
            .contains("repeated subdirectory"));

        let child_offset = (8 + entry("a", true, 0).len() * 2 + 8) as u64;
        let mut siblings = entry("a", true, child_offset);
        siblings.extend_from_slice(&entry("b", true, child_offset));
        let mut aliased = metadata(&siblings);
        aliased.extend_from_slice(&entry("file", false, 0));
        aliased.extend_from_slice(&0u64.to_le_bytes());
        assert!(parse(&aliased)
            .unwrap_err()
            .to_string()
            .contains("repeated subdirectory"));
    }

    fn directory_chain(levels: usize) -> Vec<u8> {
        let mut bytes = metadata(&[]);
        bytes.truncate(8);
        for level in 0..levels {
            let next = if level + 1 == levels {
                0
            } else {
                (bytes.len() + entry("d", true, 0).len() + 8) as u64
            };
            bytes.extend_from_slice(&entry("d", true, next));
            bytes.extend_from_slice(&0u64.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn test_directory_depth_is_bounded() {
        assert_eq!(
            parse(&directory_chain(MAX_DIRECTORY_DEPTH + 1))
                .unwrap()
                .len(),
            MAX_DIRECTORY_DEPTH + 1
        );
        let error = parse(&directory_chain(MAX_DIRECTORY_DEPTH + 2)).unwrap_err();
        assert!(error.to_string().contains("depth limit"));
    }

    #[test]
    fn test_rejects_invalid_subdirectory_offsets_and_file_children() {
        for offset in [1, 7, 16, u64::MAX, u64::MAX - 7] {
            assert!(
                parse(&metadata(&entry("dir", true, offset))).is_err(),
                "offset {offset}"
            );
        }
        assert!(parse(&metadata(&entry("file", false, 8))).is_err());
    }

    #[test]
    fn test_truncated_metadata_and_entry_lengths_are_errors() {
        let bytes = metadata(&entry("valid", false, 0));
        for len in 0..bytes.len() {
            assert!(
                parse(&bytes[..len]).is_err(),
                "accepted truncation at {len}"
            );
        }
        for len in [1, 97, u64::MAX, u64::MAX - 7] {
            let mut bad = bytes.clone();
            bad[8..16].copy_from_slice(&len.to_le_bytes());
            assert!(parse(&bad).is_err(), "length {len}");
        }
        assert!(read_directory_entries_at(&bytes, usize::MAX).is_err());
        assert!(read_directory_entries_at(&bytes, bytes.len()).is_err());
    }

    #[test]
    fn test_security_block_extents_are_checked() {
        let valid = metadata(&[]);
        for (total, count) in [(0u32, 0u32), (7, 0), (8, 1), (u32::MAX, 0), (8, u32::MAX)] {
            let mut bad = valid.clone();
            bad[..4].copy_from_slice(&total.to_le_bytes());
            bad[4..8].copy_from_slice(&count.to_le_bytes());
            assert!(parse(&bad).is_err());
        }
        // One size field followed by 8 bytes of descriptor data.
        let mut bytes = Vec::from(24u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&8u64.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&0u64.to_le_bytes());
        assert!(parse(&bytes).unwrap().is_empty());
        bytes[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(parse(&bytes).is_err());
    }

    #[test]
    fn test_name_and_alternate_stream_extents_are_checked() {
        for (field, len) in [
            (98, 1u16),
            (100, 1),
            (98, u16::MAX - 1),
            (100, u16::MAX - 1),
        ] {
            let mut bad = entry("x", false, 0);
            bad[field..field + 2].copy_from_slice(&len.to_le_bytes());
            assert!(parse(&metadata(&bad)).is_err());
        }
        let mut missing_stream = entry("x", false, 0);
        missing_stream[96..98].copy_from_slice(&1u16.to_le_bytes());
        assert!(parse(&metadata(&missing_stream)).is_err());

        // Extra streams follow the dentry and have their own inclusive length.
        let stream_start = missing_stream.len();
        missing_stream.resize(stream_start + 40, 0);
        missing_stream[stream_start..stream_start + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(parse(&metadata(&missing_stream)).is_err());
        missing_stream[stream_start..stream_start + 8].copy_from_slice(&40u64.to_le_bytes());
        assert_eq!(parse(&metadata(&missing_stream)).unwrap().len(), 1);
    }

    #[test]
    fn test_filetime_high_bit_does_not_overflow() {
        assert_eq!(filetime_to_option(0), None);
        assert_eq!(filetime_to_option(116444736000000000), None);
        assert_eq!(filetime_to_option(116444736010000000), Some(1));
        assert!(filetime_to_option(1u64 << 63).is_some());
    }
}
