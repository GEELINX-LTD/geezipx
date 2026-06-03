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

## check-bench-regression.sh

Checks Criterion comparison output for benchmark regressions.

```bash
cargo bench -p geezipx-core
bash scripts/check-bench-regression.sh
```

The script reads `target/criterion/**/change/estimates.json` and fails when
`mean.point_estimate` exceeds `GEEZIPX_BENCH_MAX_REGRESSION_PERCENT` (default:
`10`, meaning +10%). If Criterion comparison files are missing, the script skips
by default; set `GEEZIPX_BENCH_REQUIRE_COMPARISON=1` to make missing comparison
data fail the check.

```bash
GEEZIPX_BENCH_MAX_REGRESSION_PERCENT=5 \
GEEZIPX_BENCH_REQUIRE_COMPARISON=1 \
bash scripts/check-bench-regression.sh
```