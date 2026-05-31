# GeeZipX

A cross-platform compression/decompression tool built with Rust.

## Status

**Phase 1: CLI-first development** — the workspace and core library are
initialized. CLI commands are under active development.

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
./target/release/geezipx
```

## License

MIT
