# GeeZipX

[![CI](https://github.com/GEELINX-LTD/geezipx/actions/workflows/ci.yml/badge.svg)](https://github.com/GEELINX-LTD/geezipx/actions/workflows/ci.yml)
[![Audit](https://github.com/GEELINX-LTD/geezipx/actions/workflows/deny.yml/badge.svg)](https://github.com/GEELINX-LTD/geezipx/actions/workflows/deny.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/geezipx.svg)](https://crates.io/crates/geezipx)

> **A cross-platform compression and decompression CLI tool, built with Rust.**  
> One tool to compress, decompress, and inspect archives across formats.

[简体中文](README.zh-CN.md)

---

## Features

- **Multi-format** -- Compress, decompress, list, and test 30+ archive and stream format families (see table below).
- **SFX self-extracting archives** -- create ZIP SFX executables targeting Linux, Windows, or macOS with `--sfx` and `--sfx-target`.
- **Streaming I/O** -- process large files with bounded memory usage
- **Live progress bars** -- real-time speed, ETA, and per-file status on TTY
- **Cancel-safe** -- graceful Ctrl+C with partial-file cleanup; double Ctrl+C force-kill
- **Auto-format detection** -- magic-byte recognition with extension-based fallback
- **Compression levels** -- `--level 0-9` for gzip/bzip2/tar.gz/tar.bz2/xz/lzma/tar.xz; `--level 0-11` for brotli/tar.br; `--level 0-22` for zstd/tar.zst; LZH level 0-4 (lh0/lh4-lh7); lz4/tar.lz4 accept `0` or omitted level only
- **Clobber controls** -- `--no-clobber` to skip existing files, `--force` to overwrite
- **Zip Slip protection** -- blocks path-traversal attacks in all archive formats
- **JSON output** -- `list --json` for machine-readable inspection; `test --json` for programmatic integrity results
- **Shell completions** -- bash, zsh, fish, PowerShell, elvish
- **ZIP / 7z AES-256 encryption** -- create encrypted ZIP and 7z archives with `--password`, `--password-file`, or `--password-stdin`
- **7z solid compression** -- `--solid` to compress all files together for better ratios
- **7z method selection** -- `--7z-method` (lzma2, lzma, bzip2, ppmd, deflate, copy) and `--dict-size`
- **Multi-volume output** -- `--split-size` to split archives into numbered volume files
- **Encrypted archive read support** -- `list`, `decompress`, and `test` handle password-protected ZIP, 7z, and RAR archives
- **AES-256-GCM-SIV encrypted container** -- `.enc` files via Argon2id key derivation
- **ISZ compressed ISO wrapper** -- read and write compressed ISO images
- **Optical BIN extraction** -- browse ISO/UDF content from cooked or single-track Mode 1/2 images with bounded buffers
- **UU / UUE / XXE text encoding** -- decode and encode legacy text-encoded binaries
- **Cross-platform** -- Linux, macOS, Windows (3-platform CI)
- **Single binary** -- `cargo install` ready; no runtime dependencies
- **Multi-threaded compression** -- `-j`/`--jobs` for parallel compression (tar.gz via gzp/pigz-style, xz/tar.xz via liblzma, zstd/tar.zst via native zstdmt)

### Format Support

| Format | Extensions | Read | Write | Limitations |
|--------|-----------|:----:|:-----:|-------------|
| ZIP | `.zip`, `.jar`, `.war`, `.apk`, `.ipa`, `.xpi` | ✓ | ✓ | AES-256; Deflate write; Stored/Deflate/Deflate64/BZIP2/LZMA/XZ/ZSTD/PPMd read |
| ZIPX | `.zipx` | ✓ | ✓ | XZ (method 95) write; same readers as ZIP; proprietary methods not guaranteed |
| TAR | `.tar` | ✓ | ✓ | |
| GZIP / TAR.GZ | `.gz`, `.gzip`, `.tar.gz`, `.tgz` | ✓ | ✓ | level 0-9; tar.gz uses gzp parallel engine |
| BZIP2 / TAR.BZ2 | `.bz2`, `.bzip2`, `.tar.bz2`, `.tbz`, `.tbz2` | ✓ | ✓ | level 0-9 |
| Brotli / TAR.BR | `.br`, `.brotli`, `.tar.br` | ✓ | ✓ | level 0-11 |
| LZ4 / TAR.LZ4 | `.lz4`, `.tar.lz4` | ✓ | ✓ | level 0 only (store) |
| ZSTD / TAR.ZST | `.zst`, `.zstd`, `.tar.zst`, `.tzst` | ✓ | ✓ | level 0-22; native zstdmt multi-thread |
| XZ / TAR.XZ | `.xz`, `.tar.xz`, `.txz` | ✓ | ✓ | level 0-9 |
| LZMA | `.lzma` | ✓ | ✓ | level 0-9 |
| LZ / Lzip | `.lz` | ✓ | ✓ | LZMA container with CRC-32 |
| PMA | `.pma` | ✗ | ✓ | PMarc pm0/pm1/pm2, streaming and CRC16 verification |
| TLZ | `.tlz`, `.tar.lzma`, `.tar.lz` | ✗ | ✓ | TAR wrapped in LZMA Alone or lzip; streaming |
| 7Z | `.7z` | ✓ | ✓ | AES-256 encrypt; solid mode; LZMA2/LZMA/BZIP2/PPMD/DEFLATE |
| ISO 9660 | `.iso` | ✓ | ✓ | Level 1 + Joliet/Rock Ridge; disk-backed creation |
| UDF | `.udf` | ✓ | ✓ | UDF 1.02 creation with CS0 Unicode; disk-backed, bounded file-data buffers |
| ZPAQ | `.zpaq`, `.zpq` | ✓ | Partial | streaming segments; journal listing only; models 1–3, levels 4/5 use model 3 |
| LZH / LHA | `.lzh`, `.lha` | ✓ | ✓ | lh0/lh4–lh7 write (level 0-4); bounded staging; CRC-16 verify |
| CPIO | `.cpio` | ✓ | ✓ | newc/odc |
| ASAR | `.asar` | ✓ | ✓ | Electron archive; no encrypt |
| CAB | `.cab` | ✓ | ✓ | single-volume only; no encrypt |
| DEB | `.deb` | ✓ | ✓ | data.tar\* payload; no encrypt |
| WIM | `.wim` | ✓ | ✓ | Uncompressed write; XPRESS read; LZX interoperability unresolved; LZMS and split SWM unsupported |
| ISZ | `.isz` | ✓ | ✓ | block-bounded ISO wrapper; lists/extracts ISO files; `decompress --stdout` restores padded ISO; no encryption/segments |
| RAR | `.rar` | ✓ | ✗ | read-only (licensing limitation); decryption supported |
| AES encrypted | `.enc` | ✓ | ✓ | AES-256-GCM-SIV + Argon2id; authenticated container; buffered processing |
| EGG | `.egg` | ✗ | Partial | Single-volume, non-solid, unencrypted Store/Deflate/BZip2/LZMA; streaming block CRC checks |
| Compound / MSI | `.msi`, `.msp`, `.mst`, `.cfb`, `.ole` | ✗ | ✓ | Raw storage streams; seek-based; no MSI database reconstruction or recursive CAB extraction |
| AES Crypt | `.aes` | ✗ | ✓ | v0–v3 via aescrypt-rs; bounded buffers; authenticate before plaintext output |
| IMG / IMA | `.img`, `.ima` | copy | ✓ | ISO/UDF optical content; no FAT/NTFS/partitioned-disk support |
| BIN | `.bin` | copy | ✓ | ISO/UDF optical content; cooked and raw Mode 1/2 Form 1; no audio/CUE/partitioned disks |
| UU / UUE | `.uu`, `.uue` | ✓ | ✓ | text encoding/decoding |
| XXE | `.xxe` | ✓ | ✓ | text encoding/decoding |
| Z (Unix Compress) | `.Z` | ✗ | ✓ | Streaming LZW decoder (9–16 bit); no format checksum |
| ARJ | `.arj` | ✓ | ✗ | read-only via unarc-rs |
| ACE | `.ace` | ✓ | ✗ | read-only via unarc-rs |
| ARC | `.arc` | ✓ | ✗ | read-only via unarc-rs |
| ALZ | `.alz` | ✗ | Partial | Single-volume Store/Deflate, CRC and ZipCrypto; modified BZip2 and splits deferred |

> **ZIPX note**: `-f zipx` or a `.zipx` output selects streaming XZ compression (ZIP method 95), with level 0-9 and optional AES-256. ZIPX reading supports the enabled ZIP codecs; proprietary JPEG/WavPack methods remain unsupported. ZIP and ZIPX use codec-managed dictionaries and do not buffer whole input files.
> **WIM write**: The WIM writer stores data uncompressed. For compressed write, use wimlib or other tools.
> **Format limitations are documented in code comments and docs/PRD.md.**

---

## Status

Phase 1 (CLI MVP) is **complete and mature**. All formats listed in the Format Support table support their applicable subcommands. The `completions` command is also complete.
Phase 2 (Desktop GUIs) is **the active development focus**. Two independent frontends share the Rust core: the existing Tauri desktop app and **GeeZipX Native**, a new egui/eframe app with a separate design, no webview, archive browsing, drag/drop, selective extraction, ZIP/7z encryption, bounded text preview, integrity checks, task cancellation, and light/dark themes. See [`docs/GUI_MVP_PLAN.md`](docs/GUI_MVP_PLAN.md) for Tauri and [`docs/EGUI_GUI.md`](docs/EGUI_GUI.md) for the native frontend and its format scope.

| Phase | Theme | Status |
|-------|-------|--------|
| 1 | CLI MVP | **Complete** -- crates.io releases `geezipx` (v0.7.4) and `geezipx-core` are available |
| 2 | Desktop GUI (Tauri) | **In development** -- v0.7.4 includes archive browsing, drag/drop, progress reporting, selective extraction, text/hex preview, sidebar navigation, settings panel, Toast notifications, and Windows right-click context-menu integration |
| 2 | Desktop GUI (egui) | **Unreleased** -- independent native app; build with `cargo run -p geezipx-egui`; standalone three-platform build workflow |

See [`docs/GUI_MVP_PLAN.md`](docs/GUI_MVP_PLAN.md) and [`docs/EGUI_GUI.md`](docs/EGUI_GUI.md) for each frontend's features and remaining work.

### Native GUI (egui)

```sh
cargo run -p geezipx-egui
cargo run -p geezipx-egui -- /path/to/archive.zip
cargo build -p geezipx-egui --release --locked
```

The native frontend uses Rust, egui/eframe and OpenGL, with system file dialogs and an embedded Chinese font. Its compact file manager provides sortable, resizable columns, conventional mouse/keyboard selection, creation and extraction dialogs, and inline task progress; `Ctrl/Cmd+O`, `N`, `E` and `F` open archives, create, extract and search. It requires no Node.js, Tauri, WebView2 or WebKitGTK. `cargo build` defaults to core/CLI; select a GUI package explicitly. Native build artifacts include standalone executables and a separate per-user NSIS test installer for Windows; file associations remain future work. Performance improvements over Tauri have not yet been measured on the same hardware.

### Tauri GUI Settings

The desktop GUI exposes the following settings (persisted locally via the Tauri store):

| Setting | Scope | Description |
|---------|-------|-------------|
| Language | General | UI language; applied immediately after saving. |
| Default output directory | General | Base directory used to suggest compress/extract output paths. Empty = next to the source. |
| Overwrite strategy | General | `Ask each time` (prompts before overwriting), `Skip`, or `Overwrite`. |
| Behavior after completion | General | `Do nothing` or `Open output directory` after a successful task. |
| Default format / level | Compression | Pre-selected archive format and compression level. |
| Add directories recursively | Compression | When off, a folder source contributes only its immediate files (subfolders are skipped). |
| Theme | Appearance | `Follow system`, `Light`, or `Dark`. |

Unsaved setting changes are flagged; navigating away from the Settings tab prompts for confirmation before discarding.

## Install

### From source

```sh
# Clone and build
git clone https://github.com/GEELINX-LTD/geezipx.git
cd geezipx
cargo build --release

# The binary is at ./target/release/geezipx
./target/release/geezipx --version
```

### Via cargo

```sh
cargo install geezipx
```

### Pre-built binaries

For releases that include pre-built artifacts, binaries use the following names:

| Platform | Artifact |
|----------|----------|
| Linux (x86_64) | `geezipx-linux-x86_64.tar.gz` |
| macOS (x86_64) | `geezipx-macos-x86_64.tar.gz` |
| Windows (x86_64) | `geezipx-windows-x86_64.zip` |

Each artifact is published with a `.sha256` checksum file and a combined `SHA256SUMS` file for verification.

```sh
# Download and verify (Linux example)
curl -LO https://github.com/GEELINX-LTD/geezipx/releases/latest/download/geezipx-linux-x86_64.tar.gz
curl -LO https://github.com/GEELINX-LTD/geezipx/releases/latest/download/geezipx-linux-x86_64.tar.gz.sha256
shasum -a 256 -c geezipx-linux-x86_64.tar.gz.sha256
tar -xzf geezipx-linux-x86_64.tar.gz
sudo mv geezipx /usr/local/bin/
```

> **Release artifacts:** [v0.7.4](https://github.com/GEELINX-LTD/geezipx/releases/tag/v0.7.4) includes CLI archives and desktop GUI bundles for Linux, macOS, and Windows. Check each release page for its exact files and supported architectures.

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (see `.rust-toolchain.toml`)

---

## Quick Start

```sh
# Compress a file to ZIP
geezipx compress hello.txt -o hello.zip

# Compress with explicit format
geezipx compress hello.txt -f gzip -o hello.txt.gz

# Compress a directory recursively to tar.gz
geezipx compress mydir/ -r -f tar.gz -o mydir.tar.gz

# Decompress an archive (auto-detects format)
geezipx decompress hello.zip

# Decompress gzip to stdout
geezipx decompress hello.txt.gz --stdout > output.txt

# Compress with Brotli
geezipx compress hello.txt -f brotli -o hello.txt.br

# Compress with zstandard
geezipx compress hello.txt -f zst -o hello.txt.zst

# Decompress zstandard to stdout
geezipx decompress hello.txt.zst --stdout > output.txt

# Multi-threaded zstd compression (4 workers)
geezipx compress hello.txt -f zst -o hello.txt.zst -j 4

# Compress directory into tar.lz4 archive
geezipx compress mydir -r -f tar.lz4 -o mydir.tar.lz4

# Compress directory into tar.zst archive
geezipx compress mydir -r -f tar.zst -o mydir.tar.zst

# Decompress tar.zst archive
geezipx decompress mydir.tar.zst

# List contents of tar.zst archive
geezipx list mydir.tar.zst

# Decompress to a specific directory
geezipx decompress archive.tar.gz -o /tmp/out

# Decompress skipping existing files
geezipx decompress archive.zip --no-clobber

# Decompress overwriting existing files
geezipx decompress archive.zip --force

# List archive contents
geezipx list archive.zip

# List archive contents as JSON
geezipx list archive.tar.gz --json

# Test archive integrity (without extracting to disk)
geezipx test archive.zip

# Test with JSON output
geezipx test archive.tar.gz --json

# Pipe data through stdin/stdout (single-stream formats)
echo "Hello" | geezipx compress --stdin -f gz -o hello.gz
echo "Hello" | geezipx compress --stdin -f gz --stdout > hello.gz
cat hello.txt | geezipx compress --stdin -f zst -o hello.txt.zst
cat hello.txt.gz | geezipx decompress --stdin -f gz --stdout > hello.txt
cat hello.txt.gz | geezipx decompress --stdin -f gz -o outdir     # writes outdir/output
geezipx compress hello.txt -f gz --stdout > hello.gz              # file to stdout

# Tar-based pipeline examples (raw tar stdin/stdout)
cat raw.tar | geezipx compress --stdin -f tar.gz -o archive.tar.gz
tar cf - mydir/ | geezipx compress --stdin -f tar.zst -o mydir.tar.zst
cat raw.tar | geezipx compress --stdin -f tar.bz2 -o archive.tar.bz2
cat raw.tar | geezipx compress --stdin -f tar.br -o archive.tar.br
cat raw.tar | geezipx compress --stdin -f tar.lz4 -o archive.tar.lz4
geezipx decompress archive.tar.gz --stdout | tar tf -
geezipx decompress archive.tar.bz2 --stdout > raw.tar
geezipx decompress archive.tar.br --stdout > raw.tar
geezipx decompress archive.tar.lz4 --stdout > raw.tar
geezipx decompress archive.tar.xz --stdout > raw.tar

# SFX self-extracting archive (create a Linux ZIP SFX executable)
geezipx compress mydir/ -r -f zip --sfx --sfx-target linux -o myapp

# Create an encrypted AES container
geezipx compress secret.txt -f aes --password mypass -o secret.enc

# Decompress an AES container
geezipx decompress secret.enc --password mypass

# Compress a directory to ISZ (compressed ISO)
geezipx compress mydir/ -r -f isz -o disk.isz

# UUencode a file
geezipx compress data.bin -f uu -o data.uu

# Multi-volume ZIP (100 MiB per volume)
geezipx compress bigdir/ -r -f zip -o archive.zip --split-size 100M

# 7z with solid compression and LZMA2 64 MB dictionary
geezipx compress mydir/ -r -f 7z -o archive.7z --solid --dict-size 64M
```

---

## Usage

### Global Flags

| Flag | Description |
|------|-------------|
| `--no-progress` | Disable progress bar (ideal for scripts) |
| `-v`, `--verbose` | Log each file as it's processed |

### `compress` — Create archives

```sh
geezipx compress <inputs...> -o <output> [options]
```

| Option | Description |
|--------|-------------|
| `-o`, `--output` | Output file path (required unless `--stdout` is used) |
| `-f`, `--format` | Format: `zip`, `zipx`, `jar`, `war`, `apk`, `ipa`, `xpi`, `tar`, `tar.gz`, `tgz`, `tar.bz2`, `tbz`, `tbz2`, `tar.br`, `tar.lz4`, `tar.zst`, `tzst`, `tar.xz`, `txz`, `gz`, `gzip`, `bz2`, `bzip2`, `br`, `brotli`, `lz4`, `zst`, `zstd`, `xz`, `lzma`, `lz`, `7z`, `rar` (read-only), `cab`, `asar`, `deb`, `lzh`, `lha`, `iso`, `udf`, `cpio`, `zpaq`, `zpq`, `wim`, `swm`, `uu`, `uue`, `xxe`, `isz`, `aes`, `img`, `ima`, `bin` (inferred from extension if omitted, defaults to zip) |
| `-r`, `--recursive` | Recursively add directories |
| `-L`, `--level` | Compression level: 0-9 (zip/zipx/gzip/bzip2/tar.gz/tar.bz2/xz/lzma/tar.xz); 0-11 (brotli/tar.br); 0-22 (zstd/zst/tar.zst/tzst); 0-4 (LZH: 0=lh0, 1=lh4, 2=lh5, 3=lh6, 4+=lh7); lz4/tar.lz4 accept `0` or omitted only |
| `-j`, `--jobs` | Worker threads: 1 (default, single-threaded), 0 (auto), or N. Effective for tar.gz (gzp), xz/tar.xz (liblzma block parallelism), and zstd/tar.zst (zstdmt); XZ memory grows with jobs and dictionary size |
| `--password` | Encrypt ZIP or 7z with AES-256. Use `--password-file` or `--password-stdin` alternatives |
| `--7z-method` | 7z compression method: `lzma2` (default), `lzma`, `bzip2`, `ppmd`, `deflate`, `copy` |
| `--dict-size` | LZMA2 dictionary size for 7z (e.g., `16M`, `64M`, `256M`) |
| `--solid` | Enable 7z solid compression (better ratio for many small files) |
| `--no-encrypt-filenames` | Disable 7z file name encryption (default: encrypt when password set) |
| `--stdin` | Read uncompressed data from stdin (single-stream and tar-based formats; requires `--format`) |
| `--stdout` | Write compressed data to stdout (single-stream and tar-based formats; requires `--format`) |
| `--split-size` | Split output into multiple volumes (e.g., `100M`, `1G`); `.NNN` naming |
| `--sfx` | Create a self-extracting ZIP SFX executable. Mutually exclusive with `--stdout` |
| `--sfx-target` | Target platform for SFX: `linux`, `windows`, `macos` (default: host platform). Requires `--sfx` |

### `decompress` — Extract archives

```sh
geezipx decompress <archive> [options]
```

Auto-detects the format via magic bytes (with extension fallback).

| Option | Description |
|--------|-------------|
| `-o`, `--output-dir` | Output directory (default: current directory) |
| `--stdout` | Decompress to stdout. Single-stream (gzip/bzip2/brotli/lz4/zstd/xz/lzma): outputs original content. Tar-based (tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz): outputs raw tar stream. Errors on multi-file archives (zip/tar/7z/rar) |
| `--stdin` | Read compressed data from stdin (gzip/bzip2/brotli/lz4/zstd/xz/lzma and tar.gz/tar.bz2/tar.br/tar.lz4/tar.zst/tar.xz; plus lz, isz, aes, img, bin, uu, xxe; requires `--format`) |
| `-f`, `--format` | Archive/stream format (required with `--stdin`) |
| `--no-clobber` | Skip files that already exist |
| `--force` | Overwrite existing files (default; mutually exclusive with `--no-clobber`) |
| `--password` | Password for decrypting encrypted archives (ZIP AES-256, 7z AES-256, RAR). Use `--password-file` to read from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive |

### `list` — Inspect archives

```sh
geezipx list <archive> [options]
```

Displays a table of entries with path, size, compressed size, ratio, and modification time.

| Option | Description |
|--------|-------------|
| `-j`, `--json` | Output as a JSON array |
| `--password` | Password for decrypting encrypted archives (ZIP, 7z, and RAR). Use `--password-file` to read from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive |

> **Note**: Dangerous paths (absolute paths, path-traversal entries, Windows device paths) in archives emit a warning on stderr. The stdout/JSON output remains clean and unaffected.

### `test` — Verify archive integrity

```sh
geezipx test <archive> [options]
```

Reads every entry to completion without extracting to disk and reports whether
the archive is structurally sound.

Supports CRC-32 verification for zip archives.

A corrupted archive results in a non-zero exit code.

| Option | Description |
|--------|-------------|
| `-j`, `--json` | Output as JSON with `ok` boolean |
| `--password` | Password for verifying encrypted archives (ZIP AES-256, 7z AES-256, RAR). Use `--password-file` to read from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive |

### `completions` — Shell completion scripts

```sh
geezipx completions <SHELL>    # alias: geezipx comp <SHELL>
```

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

Example:
```sh
# Bash
geezipx completions bash > /usr/local/share/bash-completion/completions/geezipx

# Zsh
geezipx completions zsh > /usr/local/share/zsh/site-functions/_geezipx

# Fish
geezipx completions fish > ~/.config/fish/completions/geezipx.fish
```

After installing, restart your shell or source the file for tab-completion on subcommands, flags, and arguments.

---

## Project Structure

```text
geezipx/
├── crates/
│   ├── core/               # Shared archive engine, detection, safe I/O and progress
│   ├── cli/                # Commands, terminal output and integration tests
│   ├── gui-tauri/          # Svelte 5 frontend and Tauri Rust bridge
│   ├── gui-egui/           # Independent egui/eframe native desktop app
│   ├── sfx-stub/           # Self-extracting ZIP executable
│   └── vendor/             # Patched native dependencies
├── docs/                   # Product scope, architecture and desktop guides
├── scripts/                # Build, benchmark, interop and release helpers
└── .github/workflows/      # CI, audit, desktop builds and releases
```

The CLI and both desktop frontends depend on the shared Rust core. Archive logic lives in core; frontends handle interaction, parameter mapping and progress. The desktop apps are independent of each other.

- [Product scope and format support](docs/PRD.md)
- [Architecture and implementation limits](docs/TECH_ARCHITECTURE.md)
- [Tauri desktop guide](docs/GUI_MVP_PLAN.md) / [Native desktop guide](docs/EGUI_GUI.md)
- [Release acceptance checklist](docs/RELEASE_CHECKLIST.md)

---

## Development

### Prerequisites

- Rust stable (install via [rustup](https://rustup.rs/))
- C++ compiler toolchain with C++17 support (required for default RAR and ZPAQ support; skip them with `--no-default-features`)

### Build & Test

```sh
# Build core and CLI (default workspace members)
cargo build

# Run workspace tests without the Tauri desktop dependencies
cargo test --workspace --exclude geezipx-gui --all-features

# Release build
cargo build --release

# Check formatting
cargo fmt --all --check

# Run clippy (strict mode, without Tauri)
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings

# Generate documentation
cargo doc --no-deps
```

### C++ build dependencies

RAR and ZPAQ (read and write) require a C++17-capable compiler at build time:

```sh
# Default build (RAR + ZPAQ via C++ backends)
cargo build --release
cargo test --all-features

# Build without C++-backed features
cargo build --release --no-default-features
cargo test --no-default-features
```

> **Note**: `cargo install geezipx` includes RAR and ZPAQ by default. If you cannot satisfy the C++ compiler requirement, build with `--no-default-features`.

### SFX self-extracting archives

GeeZipX can create self-extracting ZIP executables for Linux, Windows, and macOS. An SFX archive is a native executable that embeds a ZIP payload and a stub that extracts it at runtime.

The SFX stub is built from the `crates/sfx-stub` workspace member. With the `sfx` feature (CLI default), creation requires a nonempty embedded stub for the selected target; missing targets return an error. The release workflow builds a matching native stub on each platform. ZIP payloads and extracted files are streamed through temporary files, and failed entry validation preserves existing destination files.

```sh
# Build an SFX archive for the host platform
geezipx compress mydir/ -r -f zip --sfx -o installer

# Target a specific platform
geezipx compress mydir/ -r -f zip --sfx --sfx-target windows -o installer.exe
geezipx compress mydir/ -r -f zip --sfx --sfx-target linux -o installer
geezipx compress mydir/ -r -f zip --sfx --sfx-target macos -o installer
```

SFX notes:

- Only ZIP archives can be wrapped in an SFX stub; `--sfx` implies `-f zip`.
- `--sfx` and `--stdout` are mutually exclusive.
- Build stubs from source: `cargo build -p geezipx-sfx-stub --release`, then copy the executable into the matching directory under `crates/core/stubs/` before building the CLI.

### Format-specific notes

Most format limitations are documented in the Format Support table above. Key details:

- **WIM**: The writer stores data **uncompressed** (CompressionType::None), spooling file bytes to temporary disk with incremental SHA-1. XPRESS reads are supported; LZX interoperability with wimlib samples remains unresolved and LZMS decoding is not implemented. For compressed output, use wimlib or other tools.
- **ASAR / CAB / DEB write**: All three formats now support writing. ASAR and CAB create single-volume archives; ASAR stages payloads on disk with bounded buffers and index limits, emits Electron-compatible SHA-256 integrity metadata, and verifies optional integrity during extraction/testing. DEB writes `data.tar*` payload with a `debian-binary` + stubbed `control.tar.gz`.
- **LZH/LHA write**: Supports lh0 (store) and lh4–lh7 through the maintained `geezipx-lzhuf` support library. CLI level 0 → lh0, 1 → lh4, 2 → lh5, 3 → lh6, 4+ → lh7. Inputs and candidate compressed payloads use anonymous disk staging, bounded buffers and a continuous dictionary/bitstream; compression falls back to store when it does not shrink the entry. Temporary disk usage grows with the current input and compressed payload; tmpfs consumes system RAM. Single entries >4 GiB and extended-header metadata are not supported.
- **ISO write**: Writer emits ISO 9660 Level 1 volumes with Joliet Level 3 and Rock Ridge records. File contents and the seekable image use anonymous temporary files with fixed buffers; memory scales with entry metadata. Temporary storage needs approximately the input plus image size; a memory-backed temp directory consumes system memory. POSIX attributes use creator defaults. UDF-only writing is handled by the separate `udf` format.
- **CPIO write**: Supports `newc` and `odc` variants. Does not create symlinks, devices, FIFOs, or sockets on extraction.
- **ZPAQ write**: Uses disk staging and a shared solid model with bounded 64KiB I/O. Built-in models 1–3 are supported; compatibility levels 4/5 select model 3 and do not implement zpaq CLI preprocessing methods 4/5. Metadata has a 64MiB budget and payload staging needs temporary disk space (tmpfs consumes system RAM). New segments include SHA-1 checksums, verified by path-addressed reads. Reads still load the archive and buffer decoded members; batch extraction can repeat decoding. Standard zpaq7.15 journaling archives can be listed but their logical file extraction is not yet implemented; named-segment archives created by GeeZipX are supported.
- **UDF write**: UDF 1.02 volumes use disk-backed input ranges and a seekable temporary image, then stream to the output with 64 KiB buffers. Memory scales with directory metadata; temporary storage needs approximately the input plus image size, and a memory-backed temp directory consumes system memory. The current per-file creation limit is 4 GiB. CS0 Unicode names are limited to 255 encoded bytes; existing GeeZipX images with the legacy UTF-8 filename encoding remain readable.
- **ISZ**: Block-compressed ISO wrapper. CLI lists/extracts files inside the ISO through a seekable block cache; `decompress --stdout` restores the sector-padded ISO. Encryption and split ISZ remain unsupported.
- **AES `.enc`**: AES-256-GCM-SIV encryption with Argon2id key derivation. Opening an archive authenticates it, including `list` and `test`. Buffered processing remains; streaming AES containers are rejected until fully supported.
- **BIN**: CLI extracts ISO/UDF optical files from 2048-byte cooked or 2352-byte Mode 1/Mode 2 Form 1 images. Audio/CUE/mixed tracks and partitioned disk filesystems are unsupported. Core copy helpers and creation retain explicit identity-copy behavior.
- **IMG/IMA**: Uses the same optical filesystem reader as BIN; FAT, NTFS and partitioned disks remain unsupported. Explicit stdin copy helpers retain identity behavior.
- **UU / UUE / XXE**: Legacy text-encoding formats. Both decode (list/decompress/test) and encode (compress) are supported.
- **RAR**: Read-only by licensing limitation. Decryption is supported via password flags.

The complete core/CLI pass against the Bandizip format list, including performance debt and deferred work, is recorded in [FORMAT_PARITY_AUDIT.md](docs/FORMAT_PARITY_AUDIT.md). BH, DAA 1.0, PEA and NSIS extraction remain unimplemented. ALZ, EGG, optical disk images, WIM and executable SFX have the subset limits recorded there; a matching extension alone does not imply full compatibility. The follow-up [parser and interoperability acceptance report](docs/FORMAT_PARITY_ACCEPTANCE.md) records real EGG, UltraISO ISZ and AES Crypt samples, independent tool comparisons, resource budgets and remaining cases.

### Benchmarks

Criterion benchmarks are configured and available for manual runs:

```sh
# Verify benchmarks compile
cargo bench --no-run -p geezipx-core

# Run full benchmarks
cargo bench -p geezipx-core
```

Benchmarks cover gzip throughput (4 levels x 2 sizes) and archive throughput (tar.gz, TarZst, ZIP round-trip).

> **Note**: Benchmarks are advisory only. GitHub-hosted runner variance makes hard thresholds unreliable.

### Interoperability & Coverage

```sh
# Standard checks against system tar, unzip, gzip
bash scripts/check-interop.sh

# Heavy stress mode (256 MB gzip, 1000-file tar.gz)
GEEZIPX_INTEROP_STRESS=1 bash scripts/check-interop.sh
```

Code coverage is tracked via [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) as an informational signal (no fail-under threshold). A scheduled CI workflow generates HTML and JSON reports on push to `main` and uploads them as build artifacts.

### Release Build Verification

```sh
# Full quality gate (run before committing)
cargo fmt --all --check && \
cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings && \
cargo test --workspace --exclude geezipx-gui --all-features && \
cargo build --release --workspace --exclude geezipx-gui
```

### Release Artifact Dry-Run

Before tagging a release, you can trigger the [Release workflow](.github/workflows/release.yml)
manually via **workflow_dispatch** with `dry_run: true` (default). This builds, packages,
and verifies artifacts across all three platforms without creating a GitHub Release,
allowing you to catch build issues before pushing a `v*` tag.

The workflow job summary includes artifact integrity checks (presence, size, SHA256)
and the combined `SHA256SUMS` file.

---

## Roadmap

The CLI is mature. Current work focuses on the Tauri desktop app and the independent egui app, their format adapters, and native desktop acceptance for the next release. See the [Tauri guide](docs/GUI_MVP_PLAN.md), [native guide](docs/EGUI_GUI.md), and [release checklist](docs/RELEASE_CHECKLIST.md).

Future distribution channels include Homebrew, winget and APT. Format expansion follows user requests and community feedback.

---

## Configuration

GeeZipX follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html) for config files. At present, all configuration is done through command-line flags; no config file is needed.

---

## Contributing

Contributions are welcome! Please read [AGENTS.md](AGENTS.md) for development conventions and collaboration guidelines.

Before submitting a PR:

1. Ensure tests pass: `cargo test --workspace --exclude geezipx-gui --all-features`
2. Run clippy with no warnings: `cargo clippy --workspace --exclude geezipx-gui --all-targets --all-features -- -D warnings`
3. Check formatting: `cargo fmt --all --check`
4. Update documentation if your change affects the public API or CLI behavior

---

## License

This project is licensed under the MIT License -- see [LICENSE](LICENSE) for details.
