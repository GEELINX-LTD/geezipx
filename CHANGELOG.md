# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Format support — Zstandard (single-stream)**:
  - .zst / .zstd single-file compression and decompression via `zstd` crate
  - CLI `--format zst` / `--format zstd` support; auto-detection from `.zst` / `.zstd` extension
  - Compression level `0..=22` (default: 3); `--stdout` decompress supported
  - `list` shows synthetic single-stream entries (table/json)
  - Note: `.tar.zst` currently decompresses as raw zstd stream (tar layer ignored); full tar.zst pending future work




## [0.1.0] - 2026-06-01

### Added

- **Core compression/decompression engine** (`geezipx-core` crate):
- **`list` output enhancement**: table now includes `Ratio` and `Modified` columns;
  JSON output includes `compression_ratio` and `modified` fields.
  Ratio shown with 1 decimal place, modified time as UTC `YYYY-MM-DD HH:MM:SS`.
  For gzip entries with unknown original size or modification time:
  table shows `-`, JSON outputs `null`.
  - ZIP archive reading and writing via the `zip` crate
  - TAR archive reading and writing via the `tar` crate
  - GZIP single-stream compression and decompression via `flate2`
  - TAR.GZ compound archive support (tar layer over gzip compression)
  - Unified `ArchiveReader` and `ArchiveWriter` traits for format-agnostic processing
  - Automatic format detection from magic bytes (ZIP, gzip, zstd, xz) with extension-based fallback
    (zstd and xz are detect-only; decompression not yet supported)
  - Structured error types (`GeeZipError`) covering I/O, format, cancellation, path traversal, and clobber scenarios
  - Cross-platform filesystem utilities (long paths, Unicode filenames, symlinks, permission handling)
  - Zip Slip path traversal protection on archive extraction

- **CLI binary** (`geezipx` crate):
  - Three subcommands: `compress`, `decompress`, `list`
  - `compress` — create archives from files and directories; supports `--format`, `--output`, `--recursive`, `--level`
  - `decompress` — extract archives with automatic format detection; supports `--output-dir`, `--stdout`, `--no-clobber`, `--force`
  - `list` — enumerate archive entries in table or JSON (`--json`) format
  - Help documentation via `--help` on all subcommands
  - Shell completion generation for bash, zsh, fish, PowerShell, and elvish
    (`geezipx completions <SHELL>`, alias `geezipx comp`)

- **Progress and streaming**:
  - Streaming I/O wrappers (`ProgressReader`, `ProgressWriter`) for bounded-memory processing of large files
  - Real-time progress bar on TTY stderr via `indicatif` (determinate for known sizes, spinner for pipes)
  - `--no-progress` flag to suppress progress display
  - `--verbose` mode for per-file operation logging
  - Graceful Ctrl+C cancellation with in-flight file cleanup; double Ctrl+C forces immediate exit
  - Shared progress aggregation for multi-file operations

- **Format support**:
  - ZIP (read/write)
  - TAR (read/write)
  - TAR.GZ / TGZ (read/write)
  - GZIP / GZ (read/write)
  - Compression level control (`--level 0-9`) for gzip and tar.gz formats

- **Safety and clobber controls**:
  - `--no-clobber`: skip extraction of existing files without error
  - `--force`: explicit overwrite (default behavior, mutually exclusive with `--no-clobber`)
  - Zip Slip attack protection on all archive formats
  - Windows path compatibility (illegal character substitution, long-path prefixes)

- **Testing infrastructure**:
  - 210+ tests across unit tests and CLI integration tests (`assert_cmd` + `predicates`)
  - Round-trip tests for all supported formats (compress → list → decompress → content comparison)
  - External-tool interoperability testing (`scripts/check-interop.sh` against system `tar`, `unzip`, `gzip`)
  - Large-file streaming smoke tests (5 GB+)
  - Multi-file (10,000 entries) stress test
  - Criterion benchmarks for gzip and archive throughput

- **Continuous integration** (GitHub Actions):
  - Three-platform build/test matrix: ubuntu-latest, macos-latest, windows-latest
  - `cargo fmt --all --check` on Ubuntu
  - `cargo clippy -D warnings` on all platforms
  - `cargo test --workspace --all-features` on all platforms
  - Release build with artifact upload (`actions/upload-artifact@v7`, 7-day retention)
  - `cargo-deny` security/license audit on push and PR, weekly scheduled scan
  - Interoperability test job (depends on clippy+test+build passing)
  - Benchmark compile check on every push/PR
  - Manual trigger benchmark workflow with optional filter parameter

[0.1.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.1.0

