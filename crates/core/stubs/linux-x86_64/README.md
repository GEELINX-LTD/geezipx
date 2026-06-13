# SFX Stub — Linux x86_64

The compiled `sfx-stub` binary for `x86_64-unknown-linux-gnu` should be placed here.

Build from the workspace root:
```bash
cargo build -p geezipx-sfx-stub --release --target x86_64-unknown-linux-gnu
cp target/x86_64-unknown-linux-gnu/release/sfx-stub crates/core/stubs/linux-x86_64/sfx-stub
```
