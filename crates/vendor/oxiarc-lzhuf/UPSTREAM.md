# GeeZipX LZH support library

Derived from `oxiarc-lzhuf 0.3.5` by COOLJAPAN OU (Team KitaSan), released under Apache-2.0. Upstream source: https://github.com/cool-japan/oxiarc . The original README is retained. The published crate did not include a license text file; `LICENSE` contains the standard Apache License 2.0 text.

The package is named `geezipx-lzhuf`; its Rust library name and the application's dependency alias remain `oxiarc_lzhuf` / `oxiarc-lzhuf`. Publish this support package before publishing a core version that depends on it. The explicit version+path dependency also works in published core packages once the required version exists on crates.io.

Local change: `LzhEncoder::encode_reader` consumes bounded input blocks through one continuous MSB bit writer and one persistent LZSS dictionary. It preserves canonical lh4–lh7 block framing, pads only once at EOF, and reuses the upstream block/Huffman implementations. It does not claim resumable lh1 or repair upstream slice-based `Compressor`/`StreamingLzhEncoder` APIs; GeeZipX uses the new reader API exclusively.

The support package requires `oxiarc-core >= 0.3.6` within the 0.3 series. Its streaming decoder returns an explicit unsupported-state error for future non-exhaustive `DecompressStatus` variants; the workspace lock and an independent fresh package resolution exercise the same API boundary.

Validate independently of the application workspace:

```bash
cargo fmt --manifest-path crates/vendor/oxiarc-lzhuf/Cargo.toml --check
cargo test --manifest-path crates/vendor/oxiarc-lzhuf/Cargo.toml --lib
cargo package --manifest-path crates/vendor/oxiarc-lzhuf/Cargo.toml --allow-dirty
```

The original external corpus/oracle tests and benchmark manifest targets are not copied into this support package. Application regressions use delharc, with independent 7-Zip/lhasa validation recorded in `docs/FORMAT_PARITY_ACCEPTANCE.md`.
