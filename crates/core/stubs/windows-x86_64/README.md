# SFX Stub — Windows x86_64

The compiled `sfx-stub.exe` binary for `x86_64-pc-windows-msvc` should be placed here.

Build from the workspace root:
```bash
cargo build -p geezipx-sfx-stub --release --target x86_64-pc-windows-msvc
cp target/x86_64-pc-windows-msvc/release/sfx-stub.exe crates/core/stubs/windows-x86_64/sfx-stub.exe
```
