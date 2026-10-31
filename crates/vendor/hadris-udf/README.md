# geezipx-udf

MIT-licensed GeeZipX fork of [`hadris-udf` 1.2.1](https://crates.io/crates/hadris-udf/1.2.1), upstream <https://github.com/hxyulin/hadris>. Original authorship and license are retained. Upstream documentation and original manifest are preserved as `README.upstream.md` and `UPSTREAM-Cargo.toml`.

The library target remains `hadris_udf`; GeeZipX uses an explicit package/version/path dependency so the support library can be published independently before core.

Local changes:

- `write::FileSource` is an immutable range of a shared seekable file. `SimpleFile::from_source` keeps payloads on disk; copies are bounded to 64 KiB. The original in-memory `SimpleFile::new` API remains available. When `source` is set, `data` must be empty. Source files must remain stable while formatting.
- Directory allocation uses actual encoded FID lengths, and information lengths exclude sector padding. FIDs stream without building a full directory byte buffer and carry their actual starting block location.
- File contents use multiple sector-aligned, 30-bit short allocation descriptors; layout arithmetic is checked. The application retains its 4 GiB per-file creation limit.
- OSTA CS0 filenames encode ASCII as compression 8 and other text as UTF-16BE compression 16; UTF-16 surrogate pairs decode correctly. Names exceeding 255 encoded bytes are rejected. Standard compression 8 decodes as Latin-1, including byte sequences also valid as UTF-8. Legacy UTF-8 decoding is limited to images with the old `*hadris-udf` producer and empty charset signature.
- Character-set descriptors, UDF revision suffixes and root parent references are corrected, and the formatter writes a second anchor at the end of the volume. Logical Volume Integrity implementation use contains the real file/directory counts and revision fields; descriptor CRC covers its actual length. Physical image size must be extended to the returned sector count by the seekable caller; the application does this on its temporary file before sequential output.
- `spin` requires at least 0.10.1, excluding the yanked 0.10.0 release. Registry checksums remain managed by Cargo.

The application adapter in `crates/core/src/archive/udf/writer.rs` snapshots arbitrary `Read` inputs and formats into a second anonymous temporary file. Payload memory is bounded; directory metadata grows with entry count. Temporary disk space is approximately the input plus image size, and tmpfs charges that space to RAM.

This library is excluded from the application workspace. Run its independent library tests and `cargo package` verification in addition to the application's quality gates. Before an authorized core release, publish the required `geezipx-udf` version and wait for registry availability. Testing and packaging do not publish anything.
