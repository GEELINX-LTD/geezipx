# Upstream provenance

This Apache-2.0 support library is derived from `nsis` 0.6.0, published by
ATRAPS LLC: https://crates.io/crates/nsis/0.6.0 and
https://github.com/ATRAPSLLC/nsis-rs.

The crates.io archive SHA-256 is
`b9e532a7cdb638d23e715042f8251581511c6bb8367848ea067e2526c117530d`.
The original LICENSE and NOTICE are retained, as is Cargo.toml.orig in the
repository. The latter is excluded from Cargo archives because Cargo regenerates
that reserved filename during packaging.

Local changes:

- Publish under the independent `geezipx-nsis` name and version, retaining the
  `nsis` Rust library name used by the core dependency alias.
- Provide borrowed installer metadata views over an already bounded,
  decoded header, without requiring the complete PE installer in memory.
- Expose the instruction iterator's resolved CreateDirectory/SetOutPath
  path so core can validate the original namespace without sanitizing it.
- Keep decoded-header accounting independent of caller payload budgets; retain
  the original builder's small-budget/truncated-solid behavior. Header size has
  a separate fixed 64 MiB ceiling.
- Add a Read-based NSIS BZip2 block decoder which retains block state and
  does not buffer the complete decoded installer. Existing slice APIs remain.

Core owns PE payload bounds, CRC validation, 64 MiB metadata accounting,
solid disk staging, file indexing, path validation, cancellation, output
and resource limits. It does not call the upstream whole-file extraction
API or execute an installer. Modern NSIS 3 Deflate uses flate2 in core;
legacy NSIS 2 modified Deflate is outside the supported core subset.

The library is excluded from the workspace and uses an explicit version plus
path dependency. It must be published independently before publishing a core
version that depends on it; local builds do not prove registry availability.
