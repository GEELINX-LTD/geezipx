# Native ZPAQ journal fixture

`journal-7.15.zpaq` is a real journaling archive generated from our `journal-input.txt` by the unmodified zpaq7.15 CLI sources bundled in the published zpaq_rs1.0.4 package (upstream https://github.com/turtle261/zpaq-rs; zpaq is public domain). These generated test data are released under the project's MIT license.

Build the CLI from the original `zpaq/zpaq.cpp` and `zpaq/libzpaq.cpp` with `g++ -O2 -pthread`; in a temporary directory containing the input as `input.txt`, run:

```sh
zpaq a journal.zpaq input.txt -m1 -threads 1
```

Generated names differ from their checked-in fixture names. zpaq7.15 `x` restores `input.txt` with the exact32byte source. SHA-256:

- `journal-7.15.zpaq`: `d0ac0c079489464d84502c0b731a624938be4fedfb6e06dabe457ef7e49e617c`
- `journal-input.txt`: `c5beb81c9e7f905d55bc59913728ad36820f75979bac3d73de3b29d73fc91a16`

This is a documented compatibility gap, not a passing extraction regression: current core can list the logical `input.txt` via JIDAC but its named-segment read API cannot reconstruct the journal's fragment references. CLI extraction returns nonzero (`entry 'input.txt' not found in archive`), preserving the destination. Streaming segment archives created by GeeZipX do extract successfully with both tools.

A follow-up needs journal-aware file-writer callbacks or a fragment/index reader that preserves latest-version/de-duplication semantics and core safe output. Calling native `extract` against final filesystem paths would bypass those guarantees and is not the implementation route.
