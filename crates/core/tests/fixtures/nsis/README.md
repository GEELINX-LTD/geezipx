# Independent NSIS fixtures

The seven `.bin` files are real PE installers produced by the native `makensis`
compiler. The compiler version and SHA-256, exact installer sizes/hashes and
original payload sizes/hashes are recorded in `manifest.json`. Installers are
read as data in tests; they are never executed. Payload files were created for
these tests and contain no third-party user data.

The matrix covers NSIS 3 Unicode Store and Deflate/BZip2/LZMA, with both solid
and non-solid streams for each codec. The inputs include Unicode names, an
explicit empty directory, an empty file, text, incompressible bytes and repeated
zeros. Native 7-Zip independently extracted every installer and matched the
original payload bytes. Its NSIS handler does not materialize the scripted empty
directory; `reference_empty_directory` records that limitation. GeeZipX separately
checks the explicit `CreateDirectory` instruction from the native compiler recipe.
The stub is produced by NSIS, whose license and source
are available at https://nsis.sourceforge.io/License and
https://sourceforge.net/projects/nsis/.

Regenerate under Linux with `makensis`, `7z`/`7zz` and Python 3.12:

```sh
python3 crates/core/tests/fixtures/nsis/generate.py --workdir /tmp/geezipx-nsis-fixtures
```

The recipe writes source files, `.nsi` recipes, compiler logs, reference listings,
independent extraction output, `fixtures/*.bin` and `manifest.json` to the chosen
directory. Replace the checked-in binaries and manifest together when deliberately
refreshing fixtures. File timestamps and compiler/stub versions can change
installer hashes; the original payload content remains deterministic.

Negative tests mutate these native bytes in memory and refresh the container
CRC when testing metadata rather than CRC failure. They cover truncation,
oversized headers, corruption, traversal, case aliases, delayed parent-file
conflicts, Windows device names, unresolved variables, cancellation and failed
writes. CRC-disabled installers report `crc32_verified: false` rather than
claiming a container checksum was checked.

The supported core subset is NSIS 3 Unicode with static paths relative to
`$INSTDIR`. ANSI/legacy/Park variants, BCJ-filtered LZMA, arbitrary packed/custom
stubs, runtime-generated files, plugin results, script emulation and generated
uninstaller reconstruction are not established by this corpus. Both GUIs require
separate integration. The tests do not substitute running an installer for
archive extraction.
