# SFX Stub — macOS x86_64

The compiled `sfx-stub` binary for `x86_64-apple-darwin` should be placed here.

Build from the workspace root:
```bash
cargo build -p geezipx-sfx-stub --release --target x86_64-apple-darwin
cp target/x86_64-apple-darwin/release/sfx-stub crates/core/stubs/macos-x86_64/sfx-stub
```
