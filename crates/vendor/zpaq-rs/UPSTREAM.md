# Maintained libzpaq support

This package is derived from `zpaq_rs 1.0.4` by Noah Cashin, under CC0-1.0, and retains the upstream source license and `zpaq/COPYING`. Sources were copied from the published crates.io package; upstream repository: https://github.com/turtle261/zpaq-rs . The `geezipx-zpaq` package uses an independent version while retaining the `zpaq_rs` Rust library name for the core dependency alias.

Local changes:

- Add `archive_from_reader_entries` for named, sized readers sharing one solid block, without payload or output Vec materialization. It retains model state across entries, emits SHA-1 segment checksums and checks exact input lengths.
- Poll zero-length output writes every64KiB compression step for cancellation after staging. Callback panics become I/O errors within the new API rather than crossing the C++ boundary; short and Interrupted reads work.
- Verify stored SHA-1 segment digests on path-addressed reads, resetting between segments and retaining support for legacy segments without digests. A checksum-only corruption formerly returned success and now fails before returning plaintext; decoder/digest handles use RAII on all error paths.
- Explicitly flush C++ output before returning success. Drain buffers before callbacks and make destruction noexcept even after an I/O error; final-buffer failure is a normal error.
- Preserve GCC native objects alongside release LTO bytecode for rust-lld, and correct the Windows UTF-8-to-wide allocation to include the terminating NUL.

Metadata and payload staging live in the core adapter. Library memory for built-in models1–3 depends on the model, not total payload length. Core compatibility levels4+ use model3; these are not zpaq CLI method4/5. Existing slice, journal/path and crypto APIs remain available.

Verification: standalone unit tests and package verification, core error/cancellation/solid-model tests, native zpaq CLI extraction and SHA-256 comparison, workspace gates and final native candidate checks. This support package must be published independently before publishing a core version requiring it; normal workspace development uses the path dependency.
