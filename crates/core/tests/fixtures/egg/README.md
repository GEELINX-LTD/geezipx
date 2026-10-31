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
writing these samples with GeeZipX. A separate Go implementation,
[blurfx/unegg](https://github.com/blurfx/unegg/tree/53bd143885752d1c940bb318200773f792fe93fb),
was built at commit `53bd143885752d1c940bb318200773f792fe93fb` with Go 1.24.4.
Its extracted relative filenames, sizes and SHA-256 hashes match GeeZipX for
six external Store/Deflate/BZip2/LZMA/mixed/empty archives (22 files total).
It rejects the POSIX-extra signature in `posix.egg`, so that fixture is verified
against upstream recorded contents rather than counted as a cross-tool success.

Encrypted/solid/split/AZO remain outside this reader's scope. Input checksums,
canonical output-tree digests and acceptance details are in
`docs/FORMAT_PARITY_ACCEPTANCE.md` at the repository root.
