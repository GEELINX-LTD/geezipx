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

- **Multi-format** -- ZIP, TAR, TAR.GZ/TGZ, TAR.ZST/TZST, TAR.XZ/TXZ, GZIP/GZ, Zstandard/ZST, XZ, LZMA (read/write), 7Z (read-only), and RAR (read-only). *Planned: 7Z write, LZH, ISO, ZIPX, SFX, ZPAQ, bzip2, Brotli, CAB, WIM, and more — see [docs/PRD.md](docs/PRD.md) section 5.1*
- **Streaming I/O** -- process large files with bounded memory usage
- **Live progress bars** -- real-time speed, ETA, and per-file status on TTY
- **Cancel-safe** -- graceful Ctrl+C with partial-file cleanup; double Ctrl+C force-kill
- **Auto-format detection** -- magic-byte recognition with extension-based fallback
- **Compression levels** -- `--level 0-9` for gzip/tar.gz/xz/lzma/tar.xz; `--level 0-22` for zstd/tar.zst
- **Clobber controls** -- `--no-clobber` to skip existing files, `--force` to overwrite
- **Zip Slip protection** -- blocks path-traversal attacks in all archive formats
- **JSON output** -- `list --json` for machine-readable inspection; `test --json` for programmatic integrity results
- **Shell completions** -- bash, zsh, fish, PowerShell, elvish
- **ZIP AES-256 encryption** -- create encrypted ZIP archives with `--password`, `--password-file`, or `--password-stdin`
- **Encrypted 7z/RAR read support** -- read-only `list`, `decompress`, and `test` can handle password-protected 7z/RAR archives
- **Cross-platform** -- Linux, macOS, Windows (3-platform CI)
- **Single binary** -- no runtime dependencies, `cargo install` ready
- **Multi-threaded compression** -- `-j`/`--jobs` for parallel compression (tar.gz via gzp/pigz-style, zstd/tar.zst via native zstdmt)

---

## Status

Phase 1 (CLI MVP) is **complete and mature**. The applicable subcommands are fully implemented: read/write formats support `compress`, `decompress`, `list`, and `test`; read-only 7z/RAR support `list`, `decompress`, and `test`. The `completions` command is also complete.
Phase 2 (Desktop GUI via Tauri) is **now the active development focus**.

| Phase | Theme | Status |
|-------|-------|--------|
| 1 | CLI MVP | **Complete** -- crates.io releases `geezipx` and `geezipx-core` are available |
| 2 | Desktop GUI (Tauri) | **In development** -- v0.5.0 already includes archive browsing, drag/drop, progress reporting, and selective extraction |

See [`docs/GUI_MVP_PLAN.md`](docs/GUI_MVP_PLAN.md) for detailed planning and remaining tasks.

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

> **Note:** The release workflow is configured to upload pre-built binaries for future `v*` tag releases. Existing releases may be source-only; check the specific GitHub Release page for available artifacts.

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

# Compress with zstandard
geezipx compress hello.txt -f zst -o hello.txt.zst

# Decompress zstandard to stdout
geezipx decompress hello.txt.zst --stdout > output.txt

# Multi-threaded zstd compression (4 workers)
geezipx compress hello.txt -f zst -o hello.txt.zst -j 4

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
geezipx decompress archive.tar.gz --stdout | tar tf -
geezipx decompress archive.tar.xz --stdout > raw.tar
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
| `-f`, `--format` | Format: `zip`, `tar`, `tar.gz`, `tgz`, `gz`, `gzip`, `tar.zst`, `tzst`, `zst`, `zstd`, `tar.xz`, `txz`, `xz`, `lzma` (inferred from extension if omitted, defaults to zip) |
| `-r`, `--recursive` | Recursively add directories |
| `-L`, `--level` | Compression level 0-9 (gzip/tar.gz/xz/tar.xz, default: 6); 0-22 (zstd/zst/tar.zst/tzst, default: zstd default) |
| `-j`, `--jobs` | Worker threads: 1 (default, single-threaded), 0 (auto, use all CPUs), or N (explicit). Effective for tar.gz (gzp parallel gzip) and zstd/tar.zst (native zstdmt); tar.xz/zip/xz/lzma accept but ignore for forward compat. **Note**: tar.gz `--jobs` does not apply in `--stdin` single-stream mode, only in archive mode
| `--password` | Encrypt ZIP with AES-256 (ZIP format only). Use `--password-file` to read the password from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive. For scripting, prefer `--password-file` or `--password-stdin` to avoid exposing the password in the process list |
| `--stdin` | Read uncompressed data from stdin (gzip/zstd/xz/lzma single-stream and tar.gz/tar.zst/tar.xz raw tar; requires `--format`; mutually exclusive with input files)
| `--stdout` | Write compressed data to stdout (gzip/zstd/xz/lzma single-stream and tar.gz/tar.zst/tar.xz raw tar; requires `--format`; mutually exclusive with `--output`)

### `decompress` — Extract archives

```sh
geezipx decompress <archive> [options]
```

Auto-detects the format via magic bytes (with extension fallback).

| Option | Description |
|--------|-------------|
| `-o`, `--output-dir` | Output directory (default: current directory) |
| `--stdout` | Decompress to stdout. Single-stream (gzip/zstd/xz/lzma): outputs original content. Tar-based (tar.gz/tar.zst/tar.xz): outputs raw tar stream. Errors on multi-file archives (zip/tar/7z/rar)
| `--stdin` | Read compressed data from stdin (gzip/zstd/xz/lzma and tar.gz/tar.zst/tar.xz; requires `--format`; mutually exclusive with archive file)
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
├── AGENTS.md               # AI agent collaboration guide
├── CHANGELOG.md            # Release changelog
├── Cargo.toml              # Workspace root
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── archive/    # Archive/container implementations
│   │       ├── config.rs   # Compression options (level/jobs/password)
│   │       ├── detect.rs   # Format detection (magic bytes + extension)
│   │       ├── error.rs    # Unified error types (GeeZipError)
│   │       ├── io.rs       # ProgressReader / ProgressWriter / ProgressEvent
│   │       └── test.rs     # Archive integrity helpers
│   ├── cli/
│   │   ├── src/
│   │   │   ├── commands/   # compress / decompress / list / test / completions
│   │   │   ├── render/     # Terminal progress + output rendering
│   │   │   └── signal.rs   # Ctrl+C cancellation token
│   │   └── tests/          # CLI integration + streaming smoke tests
│   └── gui-tauri/
│       ├── src/
│       │   ├── bridge.ts   # Frontend ↔ Tauri bridge types/helpers
│       │   ├── main.ts     # Current Tauri frontend logic (TypeScript/Vite)
│       │   └── style.css   # GUI styling
│       └── src-tauri/
│           ├── src/
│           │   ├── commands/
│           │   ├── lib.rs
│           │   └── state.rs
│           └── tauri.conf.json
├── docs/                   # Product and architecture documentation
├── scripts/                # Build, CI, benchmark, and interop helpers
└── .github/workflows/      # CI, audit, coverage, benchmark, and release workflows
```

### Architecture

GeeZipX follows a layered workspace architecture:

```text
┌─────────────┐  ┌─────────────────┐
│  cli (bin)  │  │  gui-tauri       │  ← Frontend layers (CLI / Tauri GUI)
└──────┬──────┘  └────────┬─────────┘
       │                  │
       └────────┬─────────┘
                │ depends on
        ┌───────▼──────────┐
        │  core (lib)       │  ← Core engine: archive/compression logic
        │  ─ pure data flow │     - no terminal/UI behavior
        │  ─ reusable API   │     - shared by CLI and GUI
        └──────────────────┘
```

The core library owns the archive-format logic via unified `ArchiveReader` / `ArchiveWriter` traits plus single-stream helpers. The CLI and Tauri GUI only handle user interaction, parameter mapping, and progress presentation.
---

## Development

### Prerequisites

- Rust stable (install via [rustup](https://rustup.rs/))
- C++ compiler toolchain (required for default RAR support; skip RAR with `--no-default-features`)

### Build & Test

```sh
# Build all workspace crates
cargo build

# Run all tests (unit + integration)
cargo test --workspace --all-features

# Release build
cargo build --release

# Check formatting
cargo fmt --all --check

# Run clippy (strict mode)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Generate documentation
cargo doc --no-deps
```

### RAR support

RAR archive support is **read-only** and **enabled by default**. The
[`unrar`](https://crates.io/crates/unrar) crate links against the RARLAB freeware
[UnRAR source](https://www.rarlab.com/rar_add.htm) (requires a C++ compiler).

```sh
# Default build (RAR included)
cargo build --release

# Run tests with all features (RAR included)
cargo test --all-features
```

If you want to build without RAR support (e.g., in an environment without a C++
compiler):

```sh
cargo build --release --no-default-features
cargo test --no-default-features
```

> **Note**: `cargo publish` and `cargo install` include RAR support by default.
> If you cannot use the C++ compiler requirement, build with `--no-default-features`.

### Benchmarks

Criterion benchmarks are configured and available for manual runs:

```sh
# Verify benchmarks compile
cargo bench --no-run -p geezipx-core

# Run full benchmarks
cargo bench -p geezipx-core
```

Benchmarks cover gzip throughput (4 levels x 2 sizes) and archive throughput (tar.gz, TarZst, ZIP round-trip).

> **Note**: Benchmarks are advisory only. GitHub-hosted runner variance makes hard thresholds unreliable. No further investment in benchmark baselines or CI performance gates is planned.
### Interoperability Tests


Code coverage is tracked via [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) as an informational signal (no fail-under threshold). A scheduled CI workflow generates HTML and JSON reports on push to `main` and uploads them as build artifacts.

```sh
# Generate coverage report (output: coverage/tarpaulin-report.html and .json)
cargo tarpaulin
```


```sh
# Standard checks against system tar, unzip, gzip
bash scripts/check-interop.sh

# Heavy stress mode (256 MB gzip, 1000-file tar.gz)
GEEZIPX_INTEROP_STRESS=1 bash scripts/check-interop.sh
```

### Release Build Verification

```sh
# Full quality gate (run before committing)
cargo fmt --all --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
cargo test --workspace --all-features && \
cargo build --release --workspace
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

### Phase 1 (CLI MVP) — Complete and Mature ✓

All core features and the applicable format-specific subcommands are implemented and verified:

- [x] ZIP / TAR / TAR.GZ / TAR.ZST / TAR.XZ / GZIP / ZSTD / XZ / LZMA read/write
- [x] 7z / RAR read-only support via `list`, `decompress`, and `test`
- [x] Streaming I/O with bounded memory usage
- [x] Progress bars with `indicatif`
- [x] Ctrl+C graceful cancellation
- [x] Auto-format detection (magic bytes + extension)
- [x] Clobber protection (`--no-clobber` / `--force`)
- [x] Zip Slip path traversal protection
- [x] Shell completions (5 shells)
- [x] `list --json` machine-readable output
- [x] `test` archive integrity verification with JSON output
- [x] 400+ tests (unit + integration + interop + streaming smoke)
- [x] 3-platform CI (Linux/macOS/Windows)
- [x] `cargo-deny` security audit
- [x] Criterion benchmarks (advisory, no hard gate)
- [x] crates.io releases
- [x] Multi-threaded compression (`-j`/`--jobs` for tar.gz, zstd/tar.zst)
- [x] ZIP AES-256 password encryption
- [x] stdin/stdout pipelines for single-stream and tar-based formats

### Phase 2 (Desktop GUI via Tauri) — Current Development (v0.5.0)

- [x] Tauri v2 project skeleton + TypeScript/Vite frontend
- [x] Core engine bridge via Tauri commands
- [x] Archive browser with file associations
- [x] Selective extraction from archives
- [x] In-app text/hex preview
- [x] Drag & drop into the app
- [x] Drag-out archive entries to the filesystem
- [x] Sidebar navigation and recent-path chips
- [x] Password prompts for encrypted archives (ZIP AES-256, 7z, RAR)
- [x] Live progress display with speed and remaining time
- [x] Cancel-safe task execution
- [x] GUI bundle CI is configured: standalone `gui-windows.yml` for Windows builds, plus `release.yml` for `.AppImage`, `.dmg`, and `.msi` artifacts
- [ ] First end-to-end tag-release verification of GUI bundles
- [ ] Window state persistence and additional polish

See [`docs/GUI_MVP_PLAN.md`](docs/GUI_MVP_PLAN.md) for detailed planning and task breakdown.

### Phase 3 (Future)

- [ ] Platform-native installers (Homebrew, winget, APT)
- **Format expansion** — phased, see [docs/PRD.md](docs/PRD.md) section 5.1 for the full target list
  - Compress: 7Z write, LZH, ISO, ZIPX, SFX, ZPAQ
  - Decompress: Brotli, bzip2, LZ4, CAB, WIM, DEB, ASAR
  - Historical/legacy: ARJ, LHA, ACE, ARC, ALZ (via adapter/evaluation)
  - Container/derived: JAR, WAR, APK, IPA, XPI (ZIP-reuse)
  - Disk images: IMG, ISZ, UDF
  - More formats driven by user requests and community feedback
---

## Configuration

GeeZipX follows the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html) for config files. At present, all configuration is done through command-line flags; no config file is needed.

---

## Contributing

Contributions are welcome! Please read [AGENTS.md](AGENTS.md) for development conventions and collaboration guidelines.

Before submitting a PR:

1. Ensure all tests pass: `cargo test --workspace --all-features`
2. Run clippy with no warnings: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. Check formatting: `cargo fmt --all --check`
4. Update documentation if your change affects the public API or CLI behavior

---

## License

This project is licensed under the MIT License -- see [LICENSE](LICENSE) for details.
