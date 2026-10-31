# AES Crypt regression fixtures

`v0-*.aes`, `v1-*.aes`, `v2-*.aes` are unchanged ciphertext from
[aescrypt-rs 0.2.0-rc.18 source archive](https://crates.io/api/v1/crates/aescrypt-rs/0.2.0-rc.18/download)
(`tests/test_data/test_vectors_v0.json` through `test_vectors_v2.json`, cases 0, 4, 5).
Upstream: https://github.com/Slurp9187/aescrypt-rs.
They use the public test password `Hello`, with plaintext of 0, 16 or 17 bytes:
empty, `0123456789ABCDEF`, or `0123456789ABCDEF0`. These are dependency
vectors, not claimed to originate from an independent implementation. All 63
vectors (21 per version) were also decrypted with official AES Crypt CLI 4.7.0
and byte-compared to GeeZipX CLI and recorded plaintext.

`official-v3.aes` was generated independently with official AES Crypt CLI 4.7.0
from our original payload `Official AES Crypt v3 fixture\n` (30 UTF-8 bytes),
password `Unicode 密码 🔑`:

```text
aescrypt -e -p 'Unicode 密码 🔑' -f -o official-v3.aes official-plaintext.txt
aescrypt -d -p 'Unicode 密码 🔑' -f -o official-v3-out.txt official-v3.aes
```

Source: https://www.aescrypt.com/download/ (Linux x86_64 CLI 4.7.0). This
fixture contains only our payload and is licensed like GeeZipX. No official
program is redistributed. v3 encryption uses random salt, so regeneration need
not yield identical ciphertext.

| File | SHA-256 |
|---|---|
| v0-0.aes | `c6d117cd7c3c38ae4d9d032ff43f647d0b7a289579c9a5b24c81f80c312a81c3` |
| v0-16.aes | `0645cad286e1817754d00a0196e17f0852d9ef5c81bb01b4710b320a5af4ec65` |
| v0-17.aes | `0c386cb9e35ceafed0470fe4fdfd6a69bf16a66d015c56d494101dbbec42cf9c` |
| v1-0.aes | `ba047b1a672b0f28d33d91196587c828fd7854ba83c644eb0b4dcbe1a14d112a` |
| v1-16.aes | `3bebb1cc32443126cffe4232eba6b68ea68c84cabfeef264060ffe90e1e6b897` |
| v1-17.aes | `82c7962e7c8afdff9bc6ee19e2f1f66b9857b340c8824ed018549637079e0cdf` |
| v2-0.aes | `c06663bd0666ad02982a82bf9f0ffaa6802514452c9728d4e654c58f7d3c1ce4` |
| v2-16.aes | `e6a16384692c174a7914aa57886a4ade6a55be6ae7e81653c8a6cccf4f8390b9` |
| v2-17.aes | `bddbf7e51a3d56f3dca84597aa5e460087f3b1506988981e461a0c6f32a81f58` |
| official-v3.aes | `266b3892385eec08a337b6652c6be90188252ea127cb45e946102b87a30bf2fd` |

Legacy v0-v2 leave the final-block length outside the MAC; malformed length
cannot be authenticated by any compatible reader. Seekable two-pass sources
must remain unchanged during authentication and output. The general Read API
stages ciphertext to disk first. Bad-password, corrupt-MAC and truncation tests
verify no bytes are appended to an existing destination buffer.

## Upstream vector license (MIT option)

MIT License

Copyright (c) 2026 Slurp9187

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
