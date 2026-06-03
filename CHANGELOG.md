# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2026-06-03

### Added

- **Release dry-run workflow**:
  - `.github/workflows/release.yml` now supports `workflow_dispatch` for artifact verification
  - Manual trigger builds all three platform artifacts, generates `.sha256` files, and creates
    a consolidated `SHA256SUMS` file with cross-platform checksum verification
  - CRLF line endings in Windows `.sha256` files are normalized before checksum validation
  - `Create Release` job strictly limited to `push` on `refs/tags/v*`; `workflow_dispatch` never
    creates a GitHub Release regardless of `dry_run` input

- **CLI `--jobs` option for Zstandard compression**:
  - `compress --jobs <N>` / `-j <N>` sets thread count for Zstd compression
  - Compatible with `.zst` single-stream and `.tar.zst` archive formats
  - Default: 0 (auto-detect CPU count via `zstdmt`)

- **Built-in glob expansion for compress inputs**:
  - CLI `compress` expands glob patterns (e.g., `*.txt`) internally before processing
  - Consistent cross-platform behavior; no longer reliant on shell glob expansion

- **Benchmark regression detection pipeline**:
  - New `scripts/setup-bench-baseline.sh` downloads the Criterion baseline artifact
    from the last successful `bench.yml` run on `main` via the GitHub CLI.
  - `bench.yml` now runs automatically on push to `main` to refresh the baseline
    artifact (in addition to the existing manual trigger).
  - New `bench-regression` job in `ci.yml` downloads the baseline, runs benchmarks,
    and checks regression thresholds using `scripts/check-bench-regression.sh`.
  - Missing baseline is handled gracefully: when no prior run exists or `gh` is
    unavailable, the download step is skipped and the regression check passes
    without failure.

- **CI improvements**:
  - New streaming smoke test job (`scripts/check-streaming-smoke.sh`) for 5GB+ file handling
  - New rustdoc warning check on Ubuntu (doc linting in CI)
  - New benchmark regression threshold check (`scripts/check-bench-regression.sh`)

- **CLI integration test coverage — XZ/LZMA single-stream formats**:
  - XZ/LZMA `--no-clobber` decompress skips existing output
  - XZ/LZMA `--force` decompress overwrites existing output
  - XZ/LZMA `compress --no-progress` suppresses ANSI escape codes on stderr
  - XZ/LZMA `compress -v` prints input filename on stderr
  - Corrupted XZ/LZMA files fail gracefully on decompress without panic
  - CLI integration coverage now includes 135 tests, adding XZ/LZMA edge cases and corrupted-input checks for remaining formats.

- **Core compression options and zstd configuration coverage**:
  - `CompressOptions::level()` returns `Some` / `None` correctly
  - `with_level()` / `with_jobs()` builder methods correctly set their own field
    without affecting the other field
  - `zstd_compress_with_options` round-trip with explicit level 9
  - `zstd_compress_with_options` multi-threaded (2 workers) round-trip

- **Compressed TAR archive cancellation coverage**:
  - `targz::extract_all_with_cancel`: basic, before-start, between-entries
  - `tarxz::extract_all_with_cancel`: basic, before-start, between-entries
  - `tarzst::extract_all_with_cancel`: basic, before-start, between-entries
  - Core lib tests grow from 249 to 258

### Changed

- **Documentation prioritized over coverage targets**:
  - `docs/PRD.md`, `docs/TECH_ARCHITECTURE.md`, `docs/PHASE1_CLI_TASKS.md` updated to reflect
    shift from coverage-driven to release-ready and product-quality focus
  - Coverage explicitly documented as informational only; no hard coverage gate or `fail-under`
  - `docs/PHASE1_CLI_TASKS.md` clarified remaining follow-ups for Phase One
  - Chinese README updated and markdown formatting artifacts fixed

### Fixed

- **CRLF normalization in release checksum verification**:
  - Windows `shasum -a 256` produces CRLF line endings; Linux `shasum -c` treats `\r` as part of
    the filename causing checksum mismatch
  - `.sha256` files now normalized to LF before generating `SHA256SUMS` and before running
    `shasum -c` validation in both `consolidate` and `release` jobs
## [0.2.1] - 2026-06-02

### Added

- **Archive format — TarXz (tar.xz / .txz)**:
  - Full round-trip archive support combining TAR format with XZ compression
  - `.tar.xz` / `.txz` files now recognized as archive format (not single-stream xz)
  - CLI `--format tar.xz` / `--format txz` support; auto-detection from `.tar.xz` / `.txz` extension
  - Automatic decompression with archive entry extraction (list, extract)
  - Compression level `0..=9` (default: 6); `--stdout` rejected (multi-file archive)
  - Note: `.xz` / `.lzma` single-stream behavior is unaffected

- **Empty directory preservation (all archive formats)**:
  - `Entry` struct now includes `is_dir` field to distinguish directories from files
  - Archive writers (ZIP, TAR, TAR.GZ, TAR.XZ, TAR.ZST) write directory entries for empty directories
  - Archive readers properly enumerate directory entries; extract creates directories on disk
  - CLI `compress -r` preserves empty subdirectories in the output archive
  - Round-trip tests cover empty directories for all five archive formats (ZIP/TAR/TAR.GZ/TAR.XZ/TAR.ZST)

- **Coverage reporting workflow**:
  - `.github/workflows/coverage.yml` runs on push/PR/weekly schedule
  - Uses `cargo-tarpaulin` generating HTML + JSON reports
  - Reports uploaded as workflow artifact (30-day retention)
  - **Informational only**; no hard coverage gate

- **Additional test coverage**:
  - Core tests grew from 224 to 239: directory writer round-trips for ZIP/TAR/TAR.GZ/TAR.XZ/TAR.ZST, cancellable extraction edge cases
  - CLI integration tests grew from 102 to 106: empty directory round-trips for ZIP/TAR/TAR.GZ/TAR.XZ/TAR.ZST, extraction edge cases

## [0.2.0] - 2026-06-02

### Added

- **Archive format — TarZst (tar.zst / .tzst)**:
  - Full round-trip archive support combining TAR format with Zstandard compression
  - `.tar.zst` / `.tzst` files now recognized as archive format (not single-stream zstd)
  - CLI `--format tar.zst` / `--format tzst` support; auto-detection from `.tar.zst` / `.tzst` extension
  - Automatic decompression with archive entry extraction (list, extract)
  - Compression level `0..=22` (default: zstd default); `--stdout` rejected (multi-file archive)
  - Note: `.zst` / `.zstd` single-stream behavior is unaffected

- **Format support — Zstandard (single-stream)**:
  - .zst / .zstd single-file compression and decompression via `zstd` crate
  - CLI `--format zst` / `--format zstd` support; auto-detection from `.zst` / `.zstd` extension
  - Compression level `0..=22` (default: 3); `--stdout` decompress supported
  - `list` shows synthetic single-stream entries (table/json)
  - Note: `.tar.zst` / `.tzst` are handled by the TarZst archive format; `.zst` / `.zstd` remain single-stream

- **Format support — XZ single-stream (.xz)**:
  - .xz single-file compression and decompression via `xz2` crate
  - CLI `--format xz` support; auto-detection from `.xz` extension
  - Compression level `0..=9` (default: 6); `--stdout` decompress supported
  - `list` shows synthetic single-stream entries (table/json)
  - Note: `.tar.xz` / `.txz` files are NOT yet handled as full archive format — decompression
    produces the underlying `.tar` stream only

- **Format support — LZMA single-stream (.lzma)**:
  - .lzma single-file compression and decompression via `xz2` crate (LZMA_Alone)
  - CLI `--format lzma` support; auto-detection from `.lzma` extension
  - Compression level `0..=9` (default: 6); `--stdout` decompress supported
  - `list` shows synthetic single-stream entries (table/json)

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
[0.2.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.2.0
[0.2.1]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.2.1
[0.2.2]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.2.2

