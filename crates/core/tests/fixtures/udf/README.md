# UDF interoperability fixtures

Generated from this project's small test payloads and distributed under the project's MIT license. Images are stored with gzip `mtime=0`; they are expanded only in tests.

## Independent producer

`genisoimage.udf.gz` was created with genisoimage 1.1.11 on Linux:

```sh
genisoimage -quiet -udf -input-charset utf-8 -o genisoimage.udf independent-source/
```

The input tree contains `empty/`, `中文目录/你好.txt` (`Chinese fixture\n`), `café.txt` (`accent fixture\n`), and `Â©.txt` (`Latin-1 ambiguity fixture\n`). These are UTF-8 source names. This is an ISO/UDF hybrid; the regression explicitly uses `UdfReader` to validate its UDF view. Neither the image nor its file identifiers were generated or rewritten by GeeZipX.

- Raw image: 878592 bytes; SHA-256 `67ebb3a32ea40ae43d58b5ebb4060c545e83ed79a28079073606d38eeaa4ce66`.
- Gzip: 3810 bytes; SHA-256 `f2bc8fb42c2f74e879424de2474ad3363b45e282ed4e6b74d9ee2dc5f58c315f`.

## Legacy producer

`legacy-utf8.udf.gz` was generated from the pre-optimization GeeZipX candidate at `b1a9a2b04ca04ff446bd0b21366fef24bb2c3675` using `compress 旧版中文.txt -f udf`. The source contains exactly `legacy` (six ASCII bytes). The baseline executable SHA-256 was `6270d2a90339a66b2e9d842d0d0cd74fc5ba324c345e34cd564725b6d6e0298a`.

This intentionally preserves the old hadris-udf 1.2.1 writer's nonstandard UTF-8 under CS0 compression ID 8 and empty charset. It tests reading existing GeeZipX images, not external interoperability. The compatibility workaround is restricted to that producer and charset signature; standard CS0/8 bytes always decode as Latin-1, including ambiguous valid UTF-8 sequences.

- Raw image: 563200 bytes; SHA-256 `3d19ca830f8cb6d8ddf5c747525f1e520464bd70713feee34c4b5cef034c7ed3`.
- Gzip: 1226 bytes; SHA-256 `e7a975f60c4a0635823bc73ae304e4100953e5a9dbc63c73a26a3a447af4b826`.

Creation timestamps embedded in the volumes are producer-generated. Re-running these commands need not reproduce identical image hashes.
