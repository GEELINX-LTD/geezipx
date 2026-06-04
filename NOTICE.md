# Legal Notices for Third-Party Dependencies

## RAR / UnRAR

GeeZipX includes **read-only** support for RAR archives via the
[`unrar`](https://crates.io/crates/unrar) crate, which links against the
**RARLAB freeware UnRAR source** (provided by [WinRAR](https://www.rarlab.com/)).

- **License**: The UnRAR source code is distributed under a permissive license
  that allows free use for **any purpose** as long as the UnRAR source is **not
  used to develop a RAR-compatible compression (writing) engine**. Since
  GeeZipX only uses UnRAR for **read-only operations** (list, extract, test),
  this does not conflict with the license terms.

- **RARLAB UnRAR License**: The full license terms can be found at:
  <https://www.rarlab.com/rar_add.htm>

- **Modification notice**: GeeZipX does **not** modify the UnRAR source code.
  The `unrar` crate bundles the original UnRAR C++ source as a vendored
  dependency and compiles it as-is during build.

- **Optional feature**: RAR support is **disabled by default** and must be
  enabled with `--features rar` at build time.

- **No RAR creation**: GeeZipX does **not** support creating RAR archives.
  The UnRAR license explicitly prohibits using its code to build a
  RAR-compatible compression engine.

## Other Dependencies

For licenses of other third-party dependencies, please refer to the
individual crate licenses (accessible via `cargo license` or browsing the
crates.io pages).

---

GeeZipX itself is distributed under the terms of the license specified in
[LICENSE](./LICENSE).
