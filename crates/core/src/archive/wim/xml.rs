//! WIM XML metadata parsing.
//!
//! The XML data resource contains image metadata (name, file count,
//! total size, timestamps).  Encoded as UTF-16 LE with BOM.
//!
//! We avoid a full XML parser in favor of simple string extraction
//! since the WIM XML schema is trivial and well-defined.

use crate::detect::ArchiveFormat;
use crate::error::{GeeZipError, GeeZipResult};

use super::header::WimHeader;
use super::resource;

/// Parsed XML data containing per-image metadata.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct XmlData {
    pub images: Vec<ImageInfo>,
}

/// Metadata for a single image within a WIM file.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ImageInfo {
    /// 1-based image index.
    pub index: u32,
    /// Human-readable image name (e.g. "Windows 10 Pro").
    pub name: String,
    /// Number of directory entries.
    pub dir_count: u64,
    /// Number of file entries.
    pub file_count: u64,
    /// Total uncompressed bytes for this image.
    pub total_bytes: u64,
    /// Creation time as Unix timestamp (seconds since epoch).
    pub creation_time: Option<u64>,
    /// Last modification time as Unix timestamp (seconds since epoch).
    pub last_modification_time: Option<u64>,
}

/// Extract the text content between an opening and closing XML tag.
fn extract_tag_content(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)?;
    Some(xml[start..start + end].to_string())
}

/// Extract a numeric attribute from an IMAGE tag.
fn extract_image_index(xml_fragment: &str) -> u32 {
    // Look for INDEX="N"
    if let Some(idx_start) = xml_fragment.find("INDEX=\"") {
        let after = &xml_fragment[idx_start + 7..];
        if let Some(idx_end) = after.find('"') {
            return after[..idx_end].parse().unwrap_or(0);
        }
    }
    0
}

/// Parse a FILETIME hex value pair (HIGHPART / LOWPART) into a Unix timestamp.
fn parse_filetime(high_str: &str, low_str: &str) -> Option<u64> {
    let high = u32::from_str_radix(high_str.trim_start_matches("0x"), 16).ok()?;
    let low = u32::from_str_radix(low_str.trim_start_matches("0x"), 16).ok()?;
    filetime_to_unix(high, low)
}

/// Convert Windows FILETIME (100-ns intervals since 1601-01-01) to Unix seconds.
fn filetime_to_unix(high: u32, low: u32) -> Option<u64> {
    let nsec: i64 = ((high as i64) << 32) | (low as i64 & 0xFFFF_FFFF);
    if nsec <= 0 {
        return None;
    }
    // Difference between 1601-01-01 and 1970-01-01 in 100-ns units
    const EPOCH_DIFF_100NS: i64 = 116444736000000000;
    let unix_100ns = nsec - EPOCH_DIFF_100NS;
    if unix_100ns <= 0 {
        return None;
    }
    Some((unix_100ns / 10_000_000) as u64)
}

impl XmlData {
    /// Parse the XML data resource from the WIM file.
    pub fn parse(file: &mut std::fs::File, header: &WimHeader) -> GeeZipResult<Self> {
        if header.xml_data.compressed_size == 0 || header.xml_data.original_size == 0 {
            return Ok(XmlData { images: Vec::new() });
        }

        let raw = resource::read_resource(
            file,
            &header.xml_data,
            header.compression_type(),
            header.chunk_size,
        )?;

        if raw.len() < 2 {
            return Err(GeeZipError::format(
                "XML data too short",
                ArchiveFormat::Wim,
            ));
        }

        // Check UTF-16LE BOM: 0xFF 0xFE
        if raw[0] != 0xFF || raw[1] != 0xFE {
            return Err(GeeZipError::format(
                "XML data missing UTF-16LE BOM",
                ArchiveFormat::Wim,
            ));
        }

        // Convert UTF-16LE bytes to a Rust String.
        let u16_data: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let xml_str = String::from_utf16(&u16_data).map_err(|e| {
            GeeZipError::format(
                format!("invalid UTF-16 in WIM XML: {e}"),
                ArchiveFormat::Wim,
            )
        })?;

        let mut images = Vec::new();

        // Split by <IMAGE ... > ... </IMAGE>
        let mut search_start = 0usize;
        while let Some(pos) = xml_str[search_start..].find("<IMAGE") {
            let tag_start = search_start + pos;
            let tag_end = match xml_str[tag_start..].find('>') {
                Some(p) => tag_start + p + 1,
                None => break,
            };
            let close_tag = match xml_str[tag_end..].find("</IMAGE>") {
                Some(p) => tag_end + p,
                None => break,
            };

            let inner = &xml_str[tag_end..close_tag];

            let index = extract_image_index(&xml_str[tag_start..tag_end]);
            let name = extract_tag_content(inner, "NAME").unwrap_or_default();
            let dir_count = extract_tag_content(inner, "DIRCOUNT")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let file_count = extract_tag_content(inner, "FILECOUNT")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let total_bytes = extract_tag_content(inner, "TOTALBYTES")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            // Parse nested CREATIONTIME / LASTMODIFICATIONTIME
            let creation_time = extract_tag_content(inner, "CREATIONTIME").and_then(|ct| {
                let high = extract_tag_content(&ct, "HIGHPART")?;
                let low = extract_tag_content(&ct, "LOWPART")?;
                parse_filetime(&high, &low)
            });

            let last_modification_time = extract_tag_content(inner, "LASTMODIFICATIONTIME")
                .and_then(|ct| {
                    let high = extract_tag_content(&ct, "HIGHPART")?;
                    let low = extract_tag_content(&ct, "LOWPART")?;
                    parse_filetime(&high, &low)
                });

            images.push(ImageInfo {
                index,
                name,
                dir_count,
                file_count,
                total_bytes,
                creation_time,
                last_modification_time,
            });

            search_start = close_tag + "</IMAGE>".len();
        }

        Ok(XmlData { images })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_conversion() {
        // 2024-01-01 00:00:00 UTC = 1704067200 Unix
        // In FILETIME: high=310677, low=1574779904
        // Actually let's just test a known value:
        // 2020-01-01 00:00:00 UTC
        // Days from 1601: 153083
        // seconds: 13220928000
        // 100-ns:  132209280000000000
        // high = 30784244, low = 2579148800
        // Nah, let's just use a simpler test.
        let ts = filetime_to_unix(0x01DBF8C3, 0x6B148000);
        // This should be a valid 2024-ish timestamp
        assert!(ts.is_some());
        let ts = ts.unwrap();
        // Should be > 1700000000 (2023-11-14) and < 1800000000 (2027-01-15)
        assert!(ts > 1700000000 && ts < 1800000000);
    }

    #[test]
    fn filetime_zero_is_none() {
        assert_eq!(filetime_to_unix(0, 0), None);
    }
}
