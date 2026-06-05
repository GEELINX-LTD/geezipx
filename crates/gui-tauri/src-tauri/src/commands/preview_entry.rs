//! `preview_entry` command — read a single entry from an archive for preview.
//!
//! Returns the entry metadata and up to 512 KB of content (for files) or
//! directory metadata (for directories).

use std::path::PathBuf;

use serde::Serialize;
use tokio::task::spawn_blocking;

use crate::commands::list::{detect_archive_format, open_reader};

/// Maximum bytes to read for a preview (512 KB).
const PREVIEW_LIMIT: usize = 512 * 1024;

/// Result of a preview operation.
#[derive(Debug, Serialize)]
pub struct PreviewResult {
    /// The entry path inside the archive.
    pub entry_path: String,
    /// Kind of entry: "dir", "text", "binary", or "error".
    pub kind: String,
    /// Human-friendly size description.
    pub size_hint: String,
    /// Preview content:
    /// - For "dir": summary of what's inside (number of entries).
    /// - For "text": UTF-8 decoded preview (truncated at PREVIEW_LIMIT).
    /// - For "binary": hex dump of first 256 bytes.
    /// - For "error": error message.
    pub content: String,
    /// Total uncompressed size of the entry.
    pub total_size: u64,
    /// Whether the content was truncated due to PREVIEW_LIMIT.
    pub truncated: bool,
}

/// Preview a single entry inside an archive.
///
/// For directories this shows the entry path only (no content).
/// For files this reads up to 512 KB.  Binary content is hex-dumped;
/// text content is returned as-is.  The kind field distinguishes the cases.
#[tauri::command]
pub async fn preview_entry(
    archive_path: String,
    entry_path: String,
    password: Option<String>,
) -> Result<PreviewResult, String> {
    let path_buf = PathBuf::from(&archive_path);
    let ep = entry_path.clone();
    let pwd = password;

    spawn_blocking(move || {
        let format = detect_archive_format(&path_buf)?;
        let mut reader = open_reader(&path_buf, format, pwd.as_deref())?;
        let entries = reader
            .entries()
            .map_err(|e| format!("Failed to read entries: {e}"))?;

        // Find the requested entry.
        let entry = entries
            .iter()
            .find(|e| e.path == ep)
            .ok_or_else(|| format!("Entry '{}' not found in archive", ep))?;

        let total_size = entry.size;
        let is_dir = entry.is_dir;

        if is_dir {
            // Count children directly under this directory.
            let prefix = if ep.ends_with('/') {
                ep.clone()
            } else {
                format!("{}/", ep)
            };
            let child_count = entries
                .iter()
                .filter(|e| e.path.starts_with(&prefix) && e.path.len() > prefix.len())
                .count();
            return Ok(PreviewResult {
                entry_path: ep.clone(),
                kind: "dir".to_string(),
                size_hint: format!("Directory, {} entry(s)", child_count),
                content: if child_count > 0 {
                    format!(
                        "Directory '{}' contains {} item(s).\nDouble-click to browse.",
                        ep, child_count
                    )
                } else {
                    format!("Empty directory '{}'", ep)
                },
                total_size,
                truncated: false,
            });
        }

        // Read entry content.
        let mut buf: Vec<u8> = Vec::with_capacity(PREVIEW_LIMIT.min(total_size as usize));
        let truncated = total_size > PREVIEW_LIMIT as u64;

        match reader.extract(entry, &mut buf) {
            Ok(bytes_read) => {
                let preview_len = bytes_read.min(PREVIEW_LIMIT as u64) as usize;
                let preview_bytes = &buf[..preview_len];

                // Try to detect if the content is text or binary.
                let (kind, content) = if is_text_content(preview_bytes) {
                    let text = String::from_utf8_lossy(preview_bytes).to_string();
                    ("text".to_string(), text)
                } else {
                    // Binary: hex dump of first 256 bytes.
                    let hex_len = preview_bytes.len().min(256);
                    let hex = hex_dump(&preview_bytes[..hex_len]);
                    ("binary".to_string(), hex)
                };

                Ok(PreviewResult {
                    entry_path: ep,
                    kind,
                    size_hint: format!("{} bytes", total_size),
                    content,
                    total_size,
                    truncated,
                })
            }
            Err(e) => Ok(PreviewResult {
                entry_path: ep,
                kind: "error".to_string(),
                size_hint: String::new(),
                content: format!("Cannot read entry: {e}"),
                total_size,
                truncated: false,
            }),
        }
    })
    .await
    .map_err(|e| format!("Internal error: {e}"))?
}

/// Heuristic: check if bytes are likely UTF-8 text.
fn is_text_content(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    // Check if the content is valid UTF-8 and doesn't contain too many
    // control characters (except common whitespace).
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return false,
    };

    // Count control characters (0x00-0x08, 0x0B, 0x0C, 0x0E-0x1F).
    let ctrl_count = text
        .chars()
        .filter(|&c| {
            let code = c as u32;
            matches!(code, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F)
        })
        .count();

    // Allow at most 1% control characters.
    ctrl_count as f64 / (text.len().max(1) as f64) < 0.01
}

/// Produce a hex dump (address + hex bytes + ASCII).
fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4);
    for (i, chunk) in bytes.chunks(16).enumerate() {
        // Address
        out.push_str(&format!("{:08x}  ", i * 16));
        // Hex bytes
        for (j, b) in chunk.iter().enumerate() {
            if j == 8 {
                out.push(' ');
            }
            out.push_str(&format!("{:02x} ", b));
        }
        // Pad last line
        let remaining = chunk.len();
        let pad = if remaining <= 8 {
            8 - remaining
        } else {
            16 - remaining
        };
        for j in 0..pad {
            if (remaining + j) == 8 {
                out.push(' ');
            }
            out.push_str("   ");
        }
        // ASCII
        out.push_str(" |");
        for b in chunk {
            if b.is_ascii_graphic() || *b == b' ' {
                out.push(*b as char);
            } else {
                out.push('.');
            }
        }
        out.push_str("|\n");
    }
    out
}
