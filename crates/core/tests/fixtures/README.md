# Codec interoperability fixtures

- `stored.arj`: stored ARJ sample from MIT-licensed `unarc-rs` 0.8.0 (`tests/arj/stored.arj`), copyright Mike Krüger and contributors; contains the upstream license. Exercises listing past payload and CRC-checked extraction.
- `license1.ace`: ACE v1 sample from `unarc-rs` 0.8.0 (`tests/ace/license1.ace`), MIT, copyright Mike Krüger and contributors; archive contains the upstream license. Used to verify header/payload traversal and ACE's complemented CRC metadata.

- `ncompress.Z`: generated with ncompress 4.2.4 `compress -c`; the 16384-byte input is `(i * 73 + i / 13) % 256` for `i=0..16384`.
- `pm1.pma`: first entry of the generated PM1 fixture from MIT-licensed `royaltm/rust-delharc`, `tests/generated/pm1/pm1.pma` (followed by a zero archive terminator). Original generated archive covers PM1 start states and was verified upstream with `pmext`. Expected first entry size 32768, CRC32 41FE9947.
- `pm2.pma`: `tests/pmarc2/comment.pma` from the same repository, generated with PMARC2 under openMSX. `HELLO.TXT`, size 128, CRC32 906330C5.

Upstream: https://github.com/royaltm/rust-delharc (MIT, copyright Roman Ajnenkiel and contributors).
