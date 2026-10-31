# geezipx-iso

MIT-licensed GeeZipX fork of [`hadris-iso` 1.2.1](https://crates.io/crates/hadris-iso/1.2.1), upstream <https://github.com/hxyulin/hadris>. Original authorship and license are retained; upstream documentation and manifest are preserved as `README.upstream.md` and `UPSTREAM-Cargo.toml`.

The library target remains `hadris_iso`. GeeZipX depends on this package explicitly (with a version and local path), rather than relying on a workspace-only registry patch that would disappear when publishing `geezipx-core`.

Local changes:

- `write::FileSource` represents an immutable range of a shared seekable file. Metadata and cloned trees hold handles/ranges instead of copies of payloads. The source file must remain stable during formatting.
- `write::File::Source` supports that range with a fixed 64 KiB copy buffer; the writer and size estimator also retain the original `File::File` API.
- Sector padding writes actual zero bytes rather than only seeking past EOF. Directory backpatching works with real files, and a freshly created image's physical length matches its volume size.
- Directory records stay within one logical sector. Metadata is laid out breadth first before payloads, with forward, individually sector-confined SUSP continuations; fixed-size reference backpatching preserves compatibility with sequential Rock Ridge readers, including libarchive and empty/long-named files.

The application adapter snapshots all `Read` inputs in one anonymous temporary file and formats into a second buffered temporary file. See `crates/core/src/archive/iso/writer.rs` for resource usage, rollback and cancellation handling. `InputFiles::from_fs` retains upstream behavior; the application uses `File::Source` directly.

This library is excluded from the application workspace to keep the upstream test/feature matrix separate. Run `cargo test --manifest-path crates/vendor/hadris-iso/Cargo.toml --lib` for upstream unit tests; application tests exercise the file-backed path. `cargo package --manifest-path crates/vendor/hadris-iso/Cargo.toml` checks its independent distribution. Before publishing a core version that uses this fork, publish the required `geezipx-iso` version first and wait for registry availability; then publish core and CLI. No registry publication is performed by building or testing the repository.
