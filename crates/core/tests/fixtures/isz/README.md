# UltraISO ISZ fixture

`ultraiso.isz` was generated locally with the official UltraISO 9.7.6.3829
trial (https://www.ezbsystems.com/ultraiso/) running in an isolated Wine prefix.
The only payload is our original UTF-8 `hello.txt`:

```text
Independent UltraISO image contents
```

The line ends with LF (36 bytes total); no vendor executable or other vendor
content is included. This fixture and its original payload are provided under
the same license as GeeZipX.

Commands (Windows paths abbreviated for readability):

```text
UltraISO.exe -silent -joliet -file source\hello.txt -output ultraiso.isz
UltraISO.exe -silent -bin2iso ultraiso.isz
```

The second command produces `ultraiso.iso` beside its source. GeeZipX's raw ISO
output was byte-compared against this official output and independently checked
with 7-Zip 25.01. The fixture uses a real 64-byte ISZ v1 header and byte-sized
block geometry; its data/table offsets include the producer's alignment gap.

- ISZ SHA-256: `01747015aca4bca5d89ebf1578ea456adab97a82afaaeccd485a31e4b1a10994` (729 bytes)
- Official ISO SHA-256: `6cbbc1ef6290593355dab5cd3fa955d280f2cb98cea6a40b0a907d6b7a848db5` (53,248 bytes)
- Official ISO CRC32: `ba0d1460`
- Payload SHA-256: `aa8ea632b219083c93ad46998bf3383d273d514f9807eb5127f6e8449e68c1b5`

The regression asserts the official ISO length/CRC and the extracted filename
and exact payload. This sample validates the default unencrypted, unsegmented
zlib path; encrypted/split and real BZip2-produced ISZ are separate cases.
