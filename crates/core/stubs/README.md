# SFX Stubs

This directory holds pre-built self-extracting ZIP stubs for each target platform.

- `linux-x86_64/sfx-stub` — Linux x86-64 stub
- `windows-x86_64/sfx-stub.exe` — Windows x86-64 stub
- `macos-x86_64/sfx-stub` — macOS x86-64 stub

Stubs are built from `crates/sfx-stub/`:

```bash
cargo build -p geezipx-sfx-stub --release --target <target>
cp target/<target>/release/sfx-stub crates/core/stubs/<target-dir>/sfx-stub
```
