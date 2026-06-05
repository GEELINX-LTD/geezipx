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

- **Multi-format** -- ZIP, TAR, TAR.GZ/TGZ, TAR.ZST/TZST, TAR.XZ/TXZ, GZIP/GZ, Zstandard/ZST, XZ, LZMA (read/write), 7Z (read-only), and RAR (read-only)
- **Streaming I/O** -- process large files with bounded memory usage
- **Live progress bars** -- real-time speed, ETA, and per-file status on TTY
- **Cancel-safe** -- graceful Ctrl+C with partial-file cleanup; double Ctrl+C force-kill
- **Auto-format detection** -- magic-byte recognition with extension-based fallback
- **Compression levels** -- `--level 0-9` for gzip/tar.gz/xz/lzma/tar.xz; `--level 0-22` for zstd/tar.zst
- **Clobber controls** -- `--no-clobber` to skip existing files, `--force` to overwrite
- **Zip Slip protection** -- blocks path-traversal attacks in all archive formats
- **JSON output** -- `list --json` for machine-readable inspection; `test --json` for programmatic integrity results
- **Shell completions** -- bash, zsh, fish, PowerShell, elvish
- **ZIP AES-256 encryption** -- `--password`, `--password-file`, or `--password-stdin` (ZIP and 7z read-only)
- **Cross-platform** -- Linux, macOS, Windows (3-platform CI)
- **Single binary** -- no runtime dependencies, `cargo install` ready
- **Multi-threaded compression** -- `-j`/`--jobs` for parallel zstd and tar.zst compression

---

## Status

Phase 1 (CLI MVP) is **complete**. All core subcommands (`compress`, `decompress`, `list`, `test`, `completions`) work for all supported formats.
Phase 2 (CLI Enhancements) is **now the active development focus**. Shipped so far: ZIP AES-256 password encryption, `--password-file`/`--password-stdin`, 7z read-only support (list/extract/test with password), and RAR read-only support (list/extract/test with password).
See [`docs/PHASE1_CLI_TASKS.md`](docs/PHASE1_CLI_TASKS.md) for the full task breakdown.
### Phase 1 — Complete
| Milestone | Theme | Status |
|-----------|-------|--------|
|-----------|-------|--------|
| M1 | Project skeleton + core engine library | **Complete** |
| M2 | CLI basic commands | **Complete** |
| M3 | Streaming / progress / polish | **Complete** |
| M4 | CI / testing / release | **Complete** |

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

# Glob pattern expansion (all .rs files in src/)
geezipx compress src/**/*.rs -f tar.gz -o src-rs.tar.gz

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
```

# Pipe data through stdin/stdout (gzip/zstd/xz/lzma only)
echo "Hello" | geezipx compress --stdin -f gz -o hello.gz
echo "Hello" | geezipx compress --stdin -f gz --stdout > hello.gz
cat hello.txt | geezipx compress --stdin -f zst -o hello.txt.zst
cat hello.txt.gz | geezipx decompress --stdin -f gz --stdout > hello.txt
cat hello.txt.gz | geezipx decompress --stdin -f gz -o outdir     # writes outdir/output
geezipx compress hello.txt -f gz --stdout > hello.gz              # file to stdout

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
|| `-o`, `--output` | Output file path (required unless `--stdout` is used) |
| `-f`, `--format` | Format: `zip`, `tar`, `tar.gz`, `tgz`, `gz`, `gzip`, `tar.zst`, `tzst`, `zst`, `zstd`, `tar.xz`, `txz`, `xz`, `lzma` (inferred from extension if omitted, defaults to zip) |
| `-r`, `--recursive` | Recursively add directories |
- `-L`, `--level` | Compression level 0-9 (gzip/tar.gz/xz/tar.xz, default: 6); 0-22 (zstd/zst/tar.zst/tzst, default: zstd default) |
| `-j`, `--jobs` | Worker threads: 1 (default, single-threaded), 0 (auto, use all CPUs), or N (explicit). Currently effective for zstd/tar.zst only; other formats accept but ignore for forward compat |
|| `--password` | Encrypt ZIP with AES-256 (ZIP format only). Use `--password-file` to read the password from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive. For scripting, prefer `--password-file` or `--password-stdin` to avoid exposing the password in the process list |
|| `--stdin` | Read uncompressed data from stdin (gzip/zstd/xz/lzma only; requires `--format`; mutually exclusive with input files) |
|| `--stdout` | Write compressed data to stdout (gzip/zstd/xz/lzma only; requires `--format`; mutually exclusive with `--output`) |

### `decompress` — Extract archives

```sh
geezipx decompress <archive> [options]
```

Auto-detects the format via magic bytes (with extension fallback).

| Option | Description |
|--------|-------------|
| `-o`, `--output-dir` | Output directory (default: current directory) |
| `--stdout` | Decompress to stdout (gzip/zstd/xz/lzma only; errors on multi-file archives like tar.gz, tar.zst, tar.xz) |
|| `--stdin` | Read compressed data from stdin (gzip/zstd/xz/lzma only; requires `--format`; mutually exclusive with archive file) |
|| `-f`, `--format` | Archive/stream format (required with `--stdin`) |
| `--no-clobber` | Skip files that already exist |
| `--force` | Overwrite existing files (default; mutually exclusive with `--no-clobber`) |
|| `--password` | Password for decrypting encrypted archives (ZIP AES-256, 7z AES-256, RAR). Use `--password-file` to read from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive |

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
|| `--password` | Password for verifying encrypted archives (ZIP AES-256, 7z AES-256, RAR). Use `--password-file` to read from a file, or `--password-stdin` to read from stdin. These three options are mutually exclusive |

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

```
geezipx/
├── AGENTS.md               # AI agent collaboration guide
├── CHANGELOG.md            # Release changelog
├── Cargo.toml              # Workspace root
├── crates/
│   ├── core/               # Compression/decompression engine library
│   │   └── src/
│   │       ├── archive/    # ZIP, TAR, TAR.GZ, TAR.ZST, TAR.XZ, GZIP, ZSTD, XZ, LZMA implementations
│   │       ├── config.rs   # Compression options (CompressOptions, --jobs/--level)
│   │       ├── detect.rs   # Format detection (magic bytes + extension)
│   │       ├── error.rs    # Unified error types (GeeZipError)
│   │       └── io.rs       # Streaming I/O wrappers (ProgressReader, etc.)
│   └── cli/                # CLI binary (clap-based, thin shell over core)
│       └── src/
│           ├── commands/   # compress / decompress / list / test
│           ├── render/     # Progress bar rendering
│           └── signal.rs   # Ctrl+C cancellation
├── docs/                   # Product & architecture documentation
├── scripts/                # Build, CI, and interoperability test scripts
├── crates/cli/tests/       # CLI integration and streaming smoke tests
├── .github/workflows/      # CI, audit, and benchmark workflows
├── deny.toml               # cargo-deny configuration
└── .rust-toolchain.toml    # Rust toolchain pin
```

### Architecture

GeeZipX follows a layered workspace architecture:

```
┌─────────────┐  ┌─────────────┐
│  cli (bin)  │  │  gui-tauri  │  ← Frontend layers (CLI / future GUI)
└──────┬──────┘  └──────┬──────┘
       │                │
       └───────┬────────┘
               │  depends on
       ┌───────▼────────┐
       │  core (lib)     │  ← Core engine: all archive/compression logic
       │  ─ pure data   │     - No I/O assumptions
       │  ─ no terminal │     - Reusable by CLI and future Tauri GUI
       └────────────────┘
```

The core library handles all format logic through unified `ArchiveReader` / `ArchiveWriter` traits. The CLI is a thin shell that handles argument parsing and progress display only. This design ensures the same compression engine will power the future Tauri desktop GUI without duplication.

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

### Running Benchmarks

```sh
# Verify benchmarks compile
cargo bench --no-run -p geezipx-core

# Run full benchmarks
cargo bench -p geezipx-core
```

Benchmarks cover gzip throughput (4 levels × 2 sizes) and archive throughput (tar.gz, TarZst, ZIP round-trip).

### Interoperability Tests

### Code Coverage

Code coverage is tracked via [cargo-tarpaulin](https://github.com/xd009642/tarpaulin).
A scheduled CI workflow (`Coverage`) generates HTML and JSON reports on every push to `main`,
every pull request, and weekly, then uploads them as build artifacts.

No hard fail-under threshold is enforced — coverage serves as an observability signal for real-risk and regression scenarios, not a standalone target.

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

### Phase 1 (CLI MVP) — Complete ✓

All core features are implemented and verified:

- [x] ZIP / TAR / TAR.GZ / TAR.ZST / TAR.XZ / GZIP / ZSTD / XZ / LZMA read/write
- [x] Streaming I/O with bounded memory usage
- [x] Progress bars with indicatif
- [x] Ctrl+C graceful cancellation
- [x] Auto-format detection (magic bytes + extension)
- [x] Clobber protection (`--no-clobber` / `--force`)
- [x] Zip Slip path traversal protection
- [x] Shell completions (5 shells)
- [x] `list --json` machine-readable output
- [x] 300+ tests (unit + integration + interop + streaming smoke)
40a:- [x] `list` dangerous path warning (stderr, doesn't pollute JSON stdout)
40b:- [x] `test` archive integrity verification (CRC-32 for ZIP, structural for TAR, JSON output)
40c:- [x] Single-stream format support for `test`: GZIP, ZSTD, XZ, LZMA
- [x] 3-platform CI (Linux/macOS/Windows)
- [x] cargo-deny security audit
- [x] Criterion benchmarks
- [x] **crates.io release**

417:> **Note**: The bench-regression job runs automatically on every PR and outputs results, but it is advisory only — GitHub-hosted runner performance variance makes hard thresholds unreliable. Logs are visible for manual review.

> **Note**: Zstandard (zst/zstd) and TarZst (tar.zst/tzst) read/write were added after Phase 1 milestones as early Phase 2 format extensions folded back into the MVP.

### Phase 2 (CLI Enhancements) — Active Development 🚀

- Multi-threaded zstd/tar.zst compression via `-j`/`--jobs` (zstd native NbWorkers) — **done**
- [x] **ZIP AES-256 password encryption** — `--password`, `--password-file`, `--password-stdin` (ZIP only)
- XZ / LZMA read/write — **done**
- TAR.ZST / TAR.XZ archive read/write — **done**
- Encrypted ZIP (AES-256)
- Volume-split archives
- 7z read-only support
- RAR read-only support
- Multi-threading for additional formats such as tar.gz, zip, and xz where practical
- True stdin pipeline support for script-heavy workflows
- Stable benchmark baselines and mandatory performance regression gates

### Phase 3 (Desktop GUI) — Future

- Tauri-based desktop application
- Drag-and-drop compression
- Task queue for batch operations
- Right-click shell integration
- Platform-native installers (Homebrew, winget, APT)

### Not Planned for Phase 1

GUI, 7z write, RAR create, cloud sync, plugin system, or auto-update -- these will be considered in later phases.

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
