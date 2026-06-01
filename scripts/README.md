# Scripts

## check-interop.sh

Runs external-tool interoperability smoke tests for zip/unzip, tar, tar.gz, and gzip.

```bash
bash scripts/check-interop.sh
```

The script builds the GeeZipX binary and exercises each format against native system
tools (`unzip`, `zip`, `tar`, `gzip`). Sections for missing tools are skipped with a
`SKIP` annotation.

### Stress mode

Set `GEEZIPX_INTEROP_STRESS=1` for heavier local smoke tests that create larger files
(256 MB gzip round-trip, 1000-file tar.gz archive with listing verification):

```bash
GEEZIPX_INTEROP_STRESS=1 bash scripts/check-interop.sh
```
