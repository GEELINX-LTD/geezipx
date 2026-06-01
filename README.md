# GeeZipX

A cross-platform compression/decompression tool built with Rust.

## Status

**Phase 1: CLI MVP complete** — the `compress`, `decompress`, and `list` subcommands
are implemented for zip, tar, tar.gz, and gzip formats.  See [`docs/PHASE1_CLI_TASKS.md`](docs/PHASE1_CLI_TASKS.md)
for the complete task breakdown.

## Project Structure

```
geezipx/
├── Cargo.toml          # Workspace root
├── crates/
│   ├── core/           # Compression/decompression engine library
│   └── cli/            # CLI binary entry point
├── docs/               # Product & architecture documentation
└── AGENTS.md           # AI agent collaboration guide
```

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain, see `.rust-toolchain.toml`)

## Quick Start

```sh
cargo build --release
./target/release/geezipx --help
```

### Examples

```sh
# Compress a file to ZIP
target/release/geezipx compress hello.txt -o hello.zip

# Compress with explicit format
target/release/geezipx compress hello.txt -f gzip -o hello.txt.gz

# Compress a directory recursively to tar.gz
target/release/geezipx compress mydir/ -r -f tar.gz -o mydir.tar.gz

# Decompress an archive (auto-detects format)
target/release/geezipx decompress hello.zip

# Decompress gzip to stdout
target/release/geezipx decompress hello.txt.gz --stdout > output.txt

# Decompress to a specific directory
target/release/geezipx decompress archive.tar.gz -o /tmp/out

# List archive contents
target/release/geezipx list archive.zip

# List archive contents as JSON
target/release/geezipx list archive.tar.gz --json
```

## License

MIT
