# External EGG regression samples

Downloaded unchanged from `alkegi/unegg` commit
`9929009ca4d010e8cd88bb796f7a1868162026d4`, `tests/data/`:
https://github.com/alkegi/unegg/tree/9929009ca4d010e8cd88bb796f7a1868162026d4/tests/data

Upstream dedicates these test data to CC0 1.0:
https://github.com/alkegi/unegg/blob/9929009ca4d010e8cd88bb796f7a1868162026d4/tests/data/README.md
https://creativecommons.org/publicdomain/zero/1.0/

- `lzma5.egg`: external LZMA blocks without an end-of-payload marker;
  replacing the known output size with `u64::MAX` rejected this valid archive.
- `posix.egg`: produced by the Android archiver according to upstream; POSIX
  metadata, nested file path and three payloads with recorded expected CRCs.
- `empty_noblock.egg`: empty file represented without a compressed block.

The regression tests verify independent expected lengths/CRCs rather than
writing these samples with GeeZipX. Additional Store/Deflate/BZip2/mixed-method
samples were checked during acceptance; encrypted/solid/split/AZO remain outside
this reader's scope. Sample checksums and acceptance details are in
`docs/FORMAT_PARITY_ACCEPTANCE.md` at the repository root.
