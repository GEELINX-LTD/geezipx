//! WIM metadata resource parsing — directory entry tree walker.
//!
//! The metadata resource contains a security block followed by a
//! depth-first directory entry tree.  We walk the tree recursively
//! and produce a flat `Vec<Entry>` with full paths.

use std::collections::HashMap;

use crate::archive::Entry;
use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::{ResourceDescriptor, WimHeader};
use super::lookup::{LookupTable, Sha1Hash};
use super::resource;

/// FILE_ATTRIBUTE_DIRECTORY constant.
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;

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
#[allow(unused_assignments)]
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

    // Build a hash → resource map from the lookup table for fast extraction.
    let mut hash_to_resource: HashMap<Sha1Hash, ResourceDescriptor> = HashMap::new();
    for sd in &lookup.file_streams {
        hash_to_resource.insert(sd.hash.clone(), sd.resource);
    }

    // ---- Parse security block ----
    if meta_bytes.len() < 8 {
        return Err(GeeZipError::format(
            "metadata too short for security block",
            ArchiveFormat::Wim,
        ));
    }
    let sec_total_len = read_u32_le(&meta_bytes, 0) as usize;
    let sec_num_entries = read_u32_le(&meta_bytes, 4) as usize;
    let mut cursor = 8usize;

    // Skip security descriptor sizes array (sec_num_entries × 8 bytes)
    cursor += sec_num_entries * 8;
    // Skip the actual security descriptor data
    cursor = sec_total_len;
    // Align to 8-byte boundary
    cursor = (cursor + 7) & !7;

    // ---- Walk root directory entries ----
    let root_entries = read_directory_entries(&meta_bytes, &mut cursor)?;

    let mut entries: Vec<Entry> = Vec::new();
    let mut path_to_resource: HashMap<String, ResourceDescriptor> = HashMap::new();

    for de in root_entries {
        collect_entries(
            &meta_bytes,
            &de,
            &hash_to_resource,
            &mut entries,
            &mut path_to_resource,
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
            // Hash present but not found in lookup — might be a hard link
            entry.size
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
    entries.push(archive_entry);

    // Recurse into subdirectory if present
    if entry.subdir_offset != 0 {
        let sub_entries = read_directory_entries_at(meta_bytes, entry.subdir_offset as usize)?;
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
            )?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Low-level entry parsing
// ---------------------------------------------------------------------------

/// Read directory entries starting at `cursor`, advancing it past the entries.
fn read_directory_entries(data: &[u8], cursor: &mut usize) -> GeeZipResult<Vec<DirEntry>> {
    let entries = read_directory_entries_at(data, *cursor)?;
    // Advance cursor to the end of the last entry read.
    // read_directory_entries_at processes entries sequentially; we need to
    // track where it stopped.  For simplicity, we sum the lengths of all
    // entries processed.
    //
    // However, the current implementation doesn't need cursor advancement
    // beyond root entries (which are only called once).  If/when multiple
    // sibling entry groups need to be read, this should be fixed.
    //
    // For now, advance past the end-of-entries marker (8 bytes of zero).
    if !entries.is_empty() {
        // Approximate: the last entry ended at some offset.  Since we don't
        // track the exact offset, just skip the end marker if present.
        if *cursor + 8 <= data.len() && read_u64_le(data, *cursor) == 0 {
            *cursor += 8;
        }
    }
    Ok(entries)
}

/// Read directory entries at a specific absolute offset. Returns entries.
#[allow(unused_assignments)]
fn read_directory_entries_at(data: &[u8], mut offset: usize) -> GeeZipResult<Vec<DirEntry>> {
    let mut entries = Vec::new();

    loop {
        if offset + 8 > data.len() {
            break;
        }

        let entry_len = read_u64_le(data, offset) as usize;
        if entry_len == 0 {
            // End-of-entries marker
            break;
        }
        if offset + 8 + entry_len > data.len() {
            break;
        }

        let entry_start = offset + 8;
        let entry_data = &data[entry_start..entry_start + entry_len];

        if entry_data.len() < 98 {
            // Entry too short to contain fixed fields
            break;
        }

        // Parse fixed fields (98 bytes)
        let attributes = read_u32_le(entry_data, 0);
        let subdir_offset = read_u64_le(entry_data, 8);
        let creation_time = read_u64_le(entry_data, 32);
        let last_access_time = read_u64_le(entry_data, 40);
        let last_write_time = read_u64_le(entry_data, 48);

        let mut hash = [0u8; 20];
        hash.copy_from_slice(&entry_data[56..76]);

        let stream_count = read_u16_le(entry_data, 92);
        let short_name_len = read_u16_le(entry_data, 94) as usize;
        let file_name_len = read_u16_le(entry_data, 96) as usize;

        // Parse variable-length file name (UTF-16LE)
        let mut var_offset = 98;
        if var_offset + file_name_len > entry_data.len() {
            break;
        }

        let file_name = decode_utf16le(&entry_data[var_offset..var_offset + file_name_len]);
        var_offset += file_name_len;

        // Skip short name if present
        if short_name_len > 0 {
            var_offset += short_name_len;
        }

        // Align to 8-byte boundary after names
        let names_end = 98 + file_name_len + short_name_len;
        let padded_end = (names_end + 7) & !7;
        var_offset = padded_end;

        // Skip alternate data streams
        for _ in 0..stream_count {
            if var_offset + 8 > entry_data.len() {
                break;
            }
            let stream_len = read_u64_le(entry_data, var_offset) as usize;
            if stream_len == 0 {
                var_offset += 8;
                continue;
            }
            var_offset += 8 + stream_len;
            var_offset = (var_offset + 7) & !7; // align
        }

        let is_dir = (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0;

        // Convert FILETIME to Unix timestamp
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

        // Advance to next entry (8-byte aligned)
        offset = entry_start + ((entry_len + 7) & !7);
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u16s)
}

fn is_zero_hash(hash: &Sha1Hash) -> bool {
    hash.0.iter().all(|&b| b == 0)
}

fn filetime_to_option(ft: u64) -> Option<u64> {
    if ft == 0 {
        return None;
    }
    let nsec: i64 = ft as i64;
    const EPOCH_DIFF_100NS: i64 = 116444736000000000;
    let unix_100ns = nsec - EPOCH_DIFF_100NS;
    if unix_100ns <= 0 {
        return None;
    }
    Some((unix_100ns / 10_000_000) as u64)
}
