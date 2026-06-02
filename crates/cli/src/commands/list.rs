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
                modified: None,
            }]
        }
        ArchiveFormat::Zstd => {
            // Zstd is a single-stream compression — produce a synthetic entry.
            let inferred_name = common::zstd_output_filename(archive);
            let compressed_size = fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
            vec![Entry {
                path: inferred_name.to_string_lossy().into_owned(),
                size: 0,
                compressed_size,
                crc32: None,
                modified: None,
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
    table.set_header(vec!["Path", "Size", "Compressed", "Ratio", "Modified"]);

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
        let ratio = if e.size > 0 && e.compressed_size > 0 {
            let pct = (e.compressed_size as f64 / e.size as f64) * 100.0;
            format!("{:.1}%", pct)
        } else {
            "-".to_string()
        };
        let modified = match e.modified {
            Some(ts) => unix_ts_to_utc_string(ts),
            None => "-".to_string(),
        };
        table.add_row(vec![&e.path, &size, &compressed, &ratio, &modified]);
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
                "compression_ratio": if e.size > 0 && e.compressed_size > 0 {
                    let pct = (e.compressed_size as f64 / e.size as f64) * 100.0;
                    serde_json::json!(((pct * 10.0).round() / 10.0))
                } else {
                    serde_json::Value::Null
                },
                "modified": e.modified.map(|ts| serde_json::json!(ts)).unwrap_or(serde_json::Value::Null),
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&list).context("serializing entries to JSON")?;
    println!("{json}");
    Ok(())
}

/// Convert a Unix timestamp (seconds since epoch) to a UTC date/time string
/// formatted as `YYYY-MM-DD HH:MM:SS`.
fn unix_ts_to_utc_string(ts: u64) -> String {
    let sec = ts % 60;
    let min = (ts / 60) % 60;
    let hour = (ts / 3600) % 24;
    let days = ts / 86_400;

    // Days-since-epoch → year-month-day (civil date).
    let is_leap = |y: u64| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let days_in_months: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut y = 1970u64;
    let mut rem = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if rem < days_in_year {
            break;
        }
        rem -= days_in_year;
        y += 1;
    }

    let mut month = 0u64;
    for (i, &dm) in days_in_months.iter().enumerate() {
        let dim = if i == 1 && is_leap(y) { 29 } else { dm };
        if rem < dim {
            month = (i + 1) as u64;
            break;
        }
        rem -= dim;
    }
    if month == 0 {
        month = 12;
        // rem already adjusted for full-year offset
    }
    let day = rem + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, month, day, hour, min, sec
    )
}
