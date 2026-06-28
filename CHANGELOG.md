# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Notes

- First end-to-end tagged release still needs real-world verification.

## [0.6.0] - 2026-06-28

### Added

- **Format support — UDF read/write**:
  - Added UDF (Universal Disk Format) read/write for DVD and Blu-ray file systems.
  - Full CLI round-trip support (`compress`, `decompress`, `list`, `test`) with GUI format metadata updated.

- **Format support — CAB write** (upgraded from read-only in 0.5.0):
  - Added `CabWriter` enabling creation of cabinet archives.
  - CLI `compress -f cab` now succeeds alongside existing `list`, `decompress`, and `test` flows.

- **Format support — ASAR write** (upgraded from read-only in 0.5.0):
  - Added `AsarWriter` enabling creation of Electron ASAR archives.
  - CLI `compress -f asar` now succeeds alongside existing `list`, `decompress`, and `test` flows.

- **Format support — LZ / Lzip single-stream**:
  - Added `.lz` / `.lzip` single-stream compression read/write via the `lzip` crate.
  - CLI `compress`, `decompress`, `list`, `test` support with automatic format detection.

- **Format support — UU / UUE (read-only)**:
  - Added Unix-to-Unix encoding `.uu` / `.uue` read-only support for legacy text-encoded binaries.
  - CLI `decompress`, `list`, and `test` supported; `compress` fails early with a read-only error.

- **Format support — XXE (read-only)**:
  - Added XXEncoded `.xxe` read-only support.
  - CLI `decompress`, `list`, and `test` supported; `compress` fails early with a read-only error.

- **Format support — ALZ, ARJ, ACE, ARC (read-only)**:
  - Added read-only support for legacy formats: ALZ (Korean ALZip), ARJ (DOS-era), ACE (WinACE), and ARC (SEA ARC).
  - CLI `list`, `decompress`, and `test` support with automatic magic-byte detection.

- **Format support — Unix compress (.Z) (read-only)**:
  - Added LZW decompression for legacy Unix `compress` (.Z) files.
  - CLI `decompress`, `list`, and `test` support with automatic format detection.

- **Format support — ISO 9660 Joliet + Rock Ridge**:
  - Extended ISO writer and reader with Joliet Unicode filename support (up to 206 characters).
  - Added Rock Ridge POSIX attribute support (permissions, ownership, timestamps).
  - Improved cross-platform interoperability for disc image creation and extraction.

- **Format support — LZH/LHA compressed write (lh4-lh7)**:
  - Extended `LzhWriter` from store-only (`-lh0-`) to Huffman-compressed entries (`-lh4-` through `-lh7-`).
  - Decompressor remains compatible with all existing LZH/LHA variants.
  - Fixed LZH bit-ordering incompatibility between oxiarc-lzhuf (LSB-first) and delharc (MSB-first).

- **Format support — CPIO write** (upgraded from read-only in 0.5.0):
  - Added `CpioWriter` enabling creation of `newc`/`odc` CPIO archives.
  - CLI `compress -f cpio` now succeeds alongside existing `list`, `decompress`, and `test` flows.

- **Format support — WIM pure Rust reader**:
  - Replaced wimlib (C library) dependency with a pure Rust WIM reader for `.wim` / `.swm` images.
  - Supports XPRESS, LZX, and LZMS decompression without native dependencies.
  - List, extract, and test support for multi-image WIM archives.

- **Format support — 7z multi-threaded LZMA2 decompression**:
  - Multi-threaded LZMA2 decompression for significantly faster extraction of large 7z archives.
  - Automatic thread detection with configurable job count.
  - 7z compression level mapping (0-9) with dictionary size optimization.

- **Format support — SFX ZIP creation**:
  - Self-extracting ZIP archive creation via CLI `--sfx` / `--sfx-target` flags.
  - Three platform targets: Linux (ELF), macOS (Mach-O), Windows (PE).
  - End-to-end test suite with stub build integration in CI.

- **Desktop GUI — Internationalization (i18n)**:
  - Chinese (zh-CN) and English (en) locale switching in the Tauri frontend.
  - Runtime locale detection with user preference persistence.

- **Desktop GUI — Task progress reporting**:
  - Progress dialog with real-time progress bar, speed, and remaining time display.
  - Cancel support integrated with the core cancellation mechanism for both compress and extract operations.

- **Desktop GUI — Sidebar navigation**:
  - Persistent sidebar with navigation between compress, extract, and archive browse views.
  - Recent file/path history with `localStorage` persistence (up to 10 entries).

- **Desktop GUI — Extraction progress dialog**:
  - Dedicated extraction dialog with per-file progress display and cancel support.
  - Smart default output directory selection based on archive file location.

- **Desktop GUI — Window state persistence**:
  - Window size and position saved and restored across sessions via `tauri-plugin-window-state`.

- **Desktop GUI — Improved drag-out UX**:
  - Dedicated drag handle column in archive browser rows for more reliable drag behavior.
  - Improved temp directory management and cleanup on drag completion.

- **Desktop GUI — Archive browser merged into extract panel**:
  - Unified archive browsing and extraction experience in a single panel view.

- **Desktop GUI — Svelte 5 frontend architecture**:
  - Frontend rewritten to Svelte 5 runes-based component architecture.
  - Improved reactivity, performance, and maintainability.
  - Resolved a11y warnings and stale element reference issues.

- **Desktop GUI — Application icons updated**:
  - High-resolution application icons for all platforms.

- **Desktop GUI — Format list synced**:
  - Updated format selection and archive detection metadata to include all new read/write formats (UDF, CAB, ASAR, LZ, CPIO, SFX, LZH compressed, and legacy read-only formats).

- **Desktop GUI — Release builds**:
  - Cross-platform desktop bundles: AppImage (Linux), DMG (macOS), MSI (Windows).
  - Automated build and publishing workflow via GitHub Actions.

### Fixed

- **LZH bit-ordering incompatibility**: Fixed incompatible bit ordering between oxiarc-lzhuf (LSB-first) and delharc (MSB-first) implementations, enabling correct compressed LZH/LHA archive creation readable by standard decompressors.
- **unrar_sys LNK2038 on Windows**: Vendored `unrar_sys` with static CRT linkage to resolve MSVC runtime library mismatch (LNK2038) on Windows builds.
- **SFX e2e test reliability**: Fixed path resolution, subprocess spawning, and temp directory cleanup in SFX end-to-end tests for consistent CI execution.
- **7z relative output path in GUI**: Fixed 7z extraction with relative output paths producing incorrect directory nesting in the archive browser.
- **GUI progress dialog flash**: Eliminated spurious dialog flash during rapid task completion.
- **GUI sidebar crash on stale references**: Fixed crash caused by stale Svelte component references in sidebar navigation after view switches.
- **GUI drag-and-drop interaction**: Fixed drag behavior to correctly drag individual files rather than the parent folder when dragging archive entries.
- **GUI drag-out folder naming and extension alignment**: Fixed inconsistent folder naming and file extension stripping in drag-out operations.
- **GUI Windows build CRT**: Added static CRT flag to GUI Windows build configuration to resolve runtime library dependencies.
- **Windows CRT mismatch in CI**: Fixed MSVC runtime library mismatch in Windows CI check workflows.

### Changed

- **Documentation comprehensive cleanup**: Removed stale items, marked completed milestones, and updated format support status across `docs/PRD.md`, `docs/TECH_ARCHITECTURE.md`, `docs/GUI_MVP_PLAN.md`, and `README.md` to reflect all new formats (UDF, CAB, ASAR, LZ, CPIO, SFX, WIM, LZH compressed, ISO Joliet/Rock Ridge, and all legacy read-only formats plus GUI progress milestones).

## [0.5.0] - 2026-06-05

### Added

- **Format support — 7z write MVP**:
  - Added core `SevenZipWriter` support backed by `sevenz-rust2`, enabling GeeZipX to create standard `.7z` archives for files, multiple inputs, recursive directories, and AES-256 password-protected archives.
  - Added CLI `compress -f 7z -o output.7z` round-trip support alongside existing `list`, `decompress`, and `test` flows, plus `--password`, `--password-file`, and `--password-stdin` for encrypted 7z creation.
  - Updated GUI format metadata and compression routing so `7z` is creatable from the Tauri app, including AES-256 password creation; current scope remains a basic writer MVP without multi-thread tuning or `tar.7z` packaging.
  - Added targeted core / CLI / GUI tests covering encrypted 7z writer round-trip behavior and password failure paths.

- **Format support — CAB (read-only)**:
  - Added `MSCF` magic / `.cab` extension detection plus a path-based core `CabReader` backed by the `cab` crate for single-volume cabinet listing, extraction, and integrity verification.
  - Added CLI `list`, `decompress`, and `test` support for `.cab`, while `compress -f cab` now fails early with a read-only error before creating or truncating the output path.

- **Format support — ISO 9660 write**:
  - Added core `IsoWriter` backed by `hadris-iso`, enabling creation of ISO 9660 Level 1 disc images with lowercase filename support.
  - Added CLI `compress -f iso -o output.iso` round-trip support alongside existing `list`, `decompress`, and `test` flows.
  - Updated GUI format metadata and compression routing so `iso` is creatable from the Tauri app.
  - Added core round-trip tests and path-safety validation tests for the writer.
  - Added GUI format metadata, archive-browser/list/extract routing, frontend `.cab` archive detection, drag-out extension stripping, and Tauri file associations for the read-only archive flow.

- **Format support — ZPAQ write**:
  - Added core `ZpaqWriter` backed by `zpaq_rs::archive_from_entries`, enabling creation of ZPAQ archives with levels 1-5.
  - Added CLI `compress -f zpaq` support alongside existing `list`, `decompress`, and `test` flows.
  - Updated GUI format metadata and compression routing so `zpaq` is creatable from the Tauri app.
  - Added core round-trip tests and path-safety validation tests for the writer.
  - Non-goals: incremental journaling, deduplication, streaming per-entry compression.

- **SFX (Self-Extracting Archive) support — ZIP MVP**:
  - New `geezipx-sfx-stub` crate and `core::sfx` module for creating self-extracting ZIP executables.
  - CLI `--sfx` / `--sfx-target` flags on `compress` command; 3 platform targets (Linux/Windows/macOS).

- **Format support — WIM read-only**:
  - Added `.wim` / `.swm` extension detection and core `WimReader` backed by `wimlib-imagex` CLI (listing) and libwim FFI (extraction).
  - CLI `list`, `decompress`, and `test` commands now support WIM archives.
  - WIM supports multiple images (MVP uses first image), XPRESS/LZX/LZMS decompression.

- **Format support — CPIO (read-only)**:
  - Added `.cpio` extension / explicit-format detection plus a path-based core `CpioReader` backed by `cpio-archive` for `newc` / `odc` listing, extraction, and integrity verification.
  - Added CLI `list`, `decompress`, and `test` support for `.cpio`; `compress -f cpio` now fails early with a read-only error before creating or truncating the output path.
  - Added GUI format metadata, archive-browser/list/test routing, frontend `.cpio` archive detection, drag-out extension stripping, and Tauri file associations for the read-only archive flow.
  - Extraction intentionally does not create host symlinks, device nodes, FIFOs, sockets, or other special filesystem objects; `bin` / `crc` variants remain out of scope for the current MVP.

- **Format support — LZH/LHA (write MVP)**:
  - Added core `LzhWriter` (in-tree, store-only level-0): file entries use `-lh0-` method, directory entries use `-lhd-`; writer buffers individual entry payloads in memory.
  - Added CLI `compress -f lzh` / `-f lha` for creating archives alongside existing `list`, `decompress`, `test` flows.
  - Added GUI format metadata, compression routing, frontend `.lzh` / `.lha` archive detection, and drag-out extension stripping so LZH/LHA archives are creatable from the Tauri app.
  - Write MVP limitations: no lh5/lh6/lh7 compressed entries, no encryption/password support, no multi-volume, no extended attributes, no level-1/2/3 extended headers, and single entry payload is limited to 4 GiB.

- **Format support — DEB (read-only)**:
  - Added `.deb` extension / explicit-format detection plus a read-only core reader that opens the package's `data.tar*` payload while intentionally ignoring `control.tar.*` scripts and metadata.
  - Added CLI `list`, `decompress`, and `test` support for `.deb`; `compress`, package writing, control extraction, encryption, and password input remain unsupported.
  - Added GUI archive-browser/list/extract routing, frontend `.deb` archive detection, drag-out extension stripping, and Tauri file associations so Debian packages open in the archive flow.

- **Format support — ASAR (read-only)**:
  - Added `.asar` extension / explicit-format detection plus a read-only core reader with path-safety checks for packed entries, `.asar.unpacked` siblings, and symlink metadata.
  - Added CLI `list`, `decompress`, and `test` support for `.asar`; `compress`, archive writing, encryption, and password input remain unsupported.
  - Added GUI archive-browser/list/extract routing, frontend `.asar` archive detection, and Tauri file associations so drag-drop and open-file flows enter the archive path.

- **Format support — bzip2 / brotli / lz4 / ZIP aliases**:
  - Added single-stream `.bz2` / `bzip2`, `.br` / `brotli`, and `.lz4` read-write support.
  - Added tar-wrapped `.tar.bz2` / `.tbz` / `.tbz2`, `.tar.br`, and `.tar.lz4` archive support.
  - Added CLI `--format br` / `brotli` / `lz4` / `tar.br` / `tar.lz4` plus raw tar `--stdin` / `--stdout` behavior for tar.br and tar.lz4.
  - Added ZIP-compatible alias parsing/detection for `.jar`, `.war`, `.apk`, `.ipa`, and `.xpi` (all routed to the ZIP reader/writer).
  - GUI format metadata, drag-out extension stripping, and compression validation now reflect the newly supported bzip2/tar.bz2/brotli/tar.br/lz4/tar.lz4 formats.

- **Desktop GUI — task progress reporting**:
  - Real-time progress events emitted from the Rust backend to the Tauri frontend
  - Frontend progress indicator with speed and remaining time display
  - Cancel button integrated with the core cancellation mechanism

- **GUI release builds**:
  - New `.github/workflows/gui-windows.yml` workflow for standalone Windows desktop bundle builds
  - `.github/workflows/release.yml` now includes cross-platform GUI bundle jobs for `.AppImage`, `.dmg`, and `.msi` artifacts

- **Desktop GUI (Tauri) — workflow usability improvements**:
  - Drag-and-drop files/archives into the app window for instant compression or extraction
  - Auto-generated default output paths after selecting source files or archives
  - Selected filenames shown as chips with `+N more` overflow indication
  - Format dropdown gracefully falls back to common formats when Tauri IPC is unavailable
  - Cancel button occupies space even when hidden, preventing layout shift during operation
  - Reveal/Open Folder buttons available after successful compression or extraction
  - Recent file/path sidebar with `localStorage` persistence (up to 10 entries)
  - Password field hints, ARIA tab semantics, and refined feedback/error styling

- **Archive Browser with file associations and selective extraction**:
  - Full archive content browser: breadcrumb navigation, double-click to enter directories,
    double-click to preview file contents (text or hex dump)
  - Checkbox multi-select with selection count display; directory-first sorting
  - Detailed entry columns: name, size, compressed size, modified time, CRC32
  - Preview panel for text files and binary hex dump; directory metadata display
  - Extract Selected: choose target directory, extract only checked entries, then Reveal
  - Overwrite existing files checkbox (default: off) for safe selective extraction
  - Drag-and-drop an archive onto the app auto-loads it in the Archive Browser
  - Recent archive chip click re-opens the archive in Browse mode
  - File extension associations: `.zip`, `.jar`, `.war`, `.apk`, `.ipa`, `.xpi`, `.tar`,
    `.tar.gz`/`.tgz`, `.tar.bz2`/`.tbz`/`.tbz2`, `.tar.zst`/`.tzst`, `.tar.xz`/`.txz`,
    `.gz`, `.bz2`, `.zst`, `.xz`, `.7z`, `.rar`
  - Single-instance support: double-clicking an associated file in the OS opens it in the
    already-running app via `tauri-plugin-single-instance`
  - `get_opened_archives`, `extract_entries`, `preview_entry` backend commands
  - 8 new path normalization unit tests

- **Drag-out support for archive entries**:
  - Archive browser rows are draggable; drag single files, folders, or multiple selected entries
  - On drag start, selected entries are safely extracted to a GeeZipX temp directory
  - `tauri-plugin-drag` (`@crabnebula/tauri-plugin-drag`) initiates system drag with real file paths
  - Temp directory cleaned up after drag completes or is cancelled (with 60s fallback)
  - `prepare_drag_entries`, `cleanup_drag_temp_dir`, `cleanup_stale_drag_temp_dirs` backend commands
  - Stale temp directories are cleaned on app startup
  - Graceful fallback to "use Extract Selected" when plugin is unavailable (e.g., browser preview)

### Notes

- GUI release builds are configured in CI. The standalone Windows workflow is ready for manual builds, and the tag-triggered `release.yml` path is configured for cross-platform GUI bundles; the first end-to-end tagged release still needs real-world verification.
- **OS-level file association**: double-clicking `.zip`/`.7z`/etc. to open GeeZipX requires a
  `tauri build` packaged app; cannot be verified in Vite dev/preview mode.
- **Drag-out**: requires a real Tauri desktop environment to verify that files correctly appear
  in Finder/Explorer/Nautilus after drag-drop. Browser preview falls back to a non-blocking hint.
- **Reveal/Open Folder**: also requires Tauri desktop context; safe fallback in preview.

## [0.4.0] - 2026-06-05

### Added

- **ZIP AES-256 password encryption**:
  - `compress --password <PASSWORD>` creates AES-256-encrypted ZIP archives
  - `decompress --password <PASSWORD>` decrypts encrypted ZIP archives
  - `test --password <PASSWORD>` verifies encrypted ZIP archive integrity
  - Using `--password`, `--password-file`, or `--password-stdin` on non-ZIP formats
    (gzip, zstd, xz, lzma) now fails with a clear error message
- **Secure password input sources**:
  - `--password-file <PATH>` reads the password from a file (trailing newline stripped)
  - `--password-stdin` reads the password from standard input (trailing newline stripped)
  - All three password sources (`--password`, `--password-file`, `--password-stdin`) are
    mutually exclusive
- **Multi-threaded tar.gz compression** (Phase 2.5):
  - `compress -j N` / `--jobs N` now enables parallel gzip for tar.gz via `gzp` crate (pigz-style)
  - `gzp` 0.11 with `deflate_rust` feature (pure Rust, no native dependency)
  - Reader side uses `flate2::read::MultiGzDecoder` for multi-member gzip compatibility
  - **Note**: `--jobs` only active in archive mode; `--stdin` single-stream mode does not benefit
  - Empty passwords are rejected with an error
- **7z read-only support**:
  - `.7z` format detection from magic bytes (`37 7A BC AF 27 1C`) and `.7z` extension
  - `geezipx list archive.7z` — enumerate 7z archive entries
  - `geezipx decompress archive.7z` — extract 7z archives preserving directory structure
  - `geezipx test archive.7z` — verify 7z archive structural integrity
  - Encrypted 7z archives are supported with `--password` / `--password-file` / `--password-stdin`
  - Read-only: 7z creation is not yet implemented
- **`list` password support**:
  - `list --password <PASSWORD>` lists encrypted ZIP and 7z archive contents
  - `list --password-file <PATH>` reads the password from a file (trailing newline stripped)
  - `list --password-stdin` reads the password from standard input
  - Using `--password`, `--password-file`, or `--password-stdin` on single-stream formats
    (gzip, zstd, xz, lzma) while listing fails with a clear error message
- **Encrypted 7z fixture tests**:
  - Core unit tests: list, extract, extract_all with correct/incorrect/no password
  - CLI integration tests: list/decompress encrypted 7z with `--password`/`--password-file`;
    list encrypted ZIP with `--password-file`/`--password-stdin`;
- **stdin/stdout pipe mode (Phase 2.5)**:
  - `compress --stdin` reads uncompressed data from stdin (gzip/bzip2/zstd/xz/lzma and tar.gz/tar.bz2/tar.zst/tar.xz — raw tar stream)
  - `compress --stdout` writes compressed data to stdout (gzip/bzip2/zstd/xz/lzma and tar.gz/tar.bz2/tar.zst/tar.xz — raw tar stream)
  - `compress file --stdout -f gz` compresses a file to stdout
  - `decompress --stdin` reads compressed data from stdin (gzip/bzip2/zstd/xz/lzma and tar.gz/tar.bz2/tar.zst/tar.xz)
  - `decompress --stdin --stdout` full pipe mode: stdin to stdout
  - `decompress --stdin -o outdir` writes decompressed output as `{outdir}/output`
  - `--stdin` and `--stdout` require explicit `--format`
  - Archive formats (zip, tar, 7z, rar) are rejected with a clear error
  - `--stdin` is mutually exclusive with input file/archive arguments
  - `--stdout` is mutually exclusive with `--output`
  - **tar-based formats now supported**: tar.gz/tar.bz2/tar.zst/tar.xz `--stdin` reads raw tar from stdin, `--stdout` outputs raw tar stream; zip/tar/7z/rar still rejected
  - `compress --stdin -f tar.gz < raw.tar` pipes raw tar through outer compression only
  - `decompress archive.tar.gz --stdout` decompresses outer layer, outputs raw tar stream
  - `decompress --stdin -f tar.gz --stdout` full tar.gz pipe mode
  - **Note**: tar.gz `--jobs` does not apply in `--stdin` mode (gzp parallel gzip is archive-mode only)
    rejection of password sources on gzip/zstd/xz/lzma when listing

- **RAR read-only support (Phase 2.6)**:
  - `.rar` format detection from magic bytes (`52 61 72 21 1A 07`) and `.rar` extension
  - `geezipx list archive.rar` — enumerate RAR archive entries (includes encrypted)
  - `geezipx decompress extract.rar` — extract RAR archives preserving directory structure
  - `geezipx test archive.rar` — verify RAR archive integrity
  - Encrypted RAR archives supported with `--password` / `--password-file` / `--password-stdin`
  - Enabled by default; can be disabled with `--no-default-features`
  - Requires a C++ compiler and the RARLAB freeware UnRAR source (linked via the
    [`unrar`](https://crates.io/crates/unrar) crate)
  - Read-only: RAR creation is not supported

## [0.3.0] - 2026-06-04

### Added

- **`list` dangerous path warning**:
  - `geezipx list` now detects archive entries with potentially unsafe paths
    (absolute paths, `../` traversal, Windows UNC/device prefixes) and prints a
    warning to stderr while still displaying the listing on stdout.
  - The warning is informational only; listing succeeds regardless.
  - JSON output (`--json`) is not affected on stdout — the warning goes to
    stderr, keeping stdout valid JSON.

- **`test` archive integrity command**:
  - `geezipx test <archive>` reads every entry to completion without writing to
    disk and reports whether the archive is structurally sound.
  - Supported formats: zip, tar, tar.gz, tar.bz2, tar.zst, tar.xz, gzip, bzip2, zstd, xz, lzma.
  - ZIP files get CRC-32 verification via the `zip` crate's internal checks.
  - `--json` output provides machine-readable results.
  - Exit code 0 on success, 1 on failure.

### Changed

- **Benchmark regression check made advisory**:
  - The `bench-regression` job still runs and outputs results on every PR.
  - Due to inherent performance variance on GitHub-hosted runners, the check uses
    `continue-on-error: true` — logs remain visible but failures do not block PRs.

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

[Unreleased]: https://github.com/GEELINX-LTD/geezipx/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.6.0
[0.5.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.5.0
[0.4.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.4.0
[0.3.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.3.0
[0.2.2]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.2.2
[0.2.1]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.2.1
[0.2.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.2.0
[0.1.0]: https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.1.0
