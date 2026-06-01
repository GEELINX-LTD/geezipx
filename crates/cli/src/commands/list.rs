//! `geezipx list` — display archive contents.

use std::fs;

use std::path::Path;

use anyhow::{Context, Result};
use comfy_table::Table;
use geezipx_core::archive::Entry;
use geezipx_core::detect::ArchiveFormat;

use super::common;

/// Execute the `list` subcommand.
pub fn execute(archive: &Path, json: bool) -> Result<()> {
    if !archive.exists() {
        anyhow::bail!("archive '{}' does not exist", archive.display());
    }

    let format = common::detect_archive_format(archive)?;

    let entries = match format {
        ArchiveFormat::Gzip => {
            // Gzip is a single-stream compression — produce a synthetic entry.
            let inferred_name = common::gzip_output_filename(archive);
            let compressed_size = fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
            vec![Entry {
                path: inferred_name.to_string_lossy().into_owned(),
                size: 0,
                compressed_size,
                crc32: None,
            }]
        }
        _ => {
            let mut reader = common::open_reader(archive, format)?;
            reader.entries().context("reading archive entries")?
        }
    };

    if json {
        print_json(&entries)?;
    } else {
        print_table(&entries, &format);
    }

    Ok(())
}

/// Print entries as a human-readable table.
fn print_table(entries: &[Entry], format: &ArchiveFormat) {
    let mut table = Table::new();
    table.set_header(vec!["Path", "Size", "Compressed"]);

    for e in entries {
        let size = if e.size > 0 {
            e.size.to_string()
        } else {
            "-".to_string()
        };
        let compressed = if e.compressed_size > 0 {
            e.compressed_size.to_string()
        } else {
            "-".to_string()
        };
        table.add_row(vec![&e.path, &size, &compressed]);
    }

    println!("{table}");
    eprintln!("Archive: {} entries (format: {})", entries.len(), format,);
}

/// Print entries as a JSON array.
fn print_json(entries: &[Entry]) -> Result<()> {
    let list: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "path": e.path,
                "compressed_size": if e.compressed_size > 0 {
                    serde_json::Value::Number(e.compressed_size.into())
                } else {
                    serde_json::Value::Null
                },
                "size": if e.size > 0 {
                    serde_json::Value::Number(e.size.into())
                } else {
                    serde_json::Value::Null
                },
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&list).context("serializing entries to JSON")?;
    println!("{json}");
    Ok(())
}
