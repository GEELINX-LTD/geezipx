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

## setup-bench-baseline.sh

Downloads the Criterion benchmark artifact from the last successful `bench.yml` run on
`main`, so that subsequent `cargo bench` produces Criterion comparison data for
regression detection.

```bash
bash scripts/setup-bench-baseline.sh
```


The script uses the GitHub CLI (`gh`) to locate and download the `criterion-report`
artifact. It is designed for CI environments where:

1. **main branch** (`bench.yml` on push to main) — runs benchmark and uploads the
   Criterion result directory as a `criterion-report` artifact.
2. **PR/CI** (`ci.yml` bench-regression job) — downloads the main-branch artifact
   as the baseline, runs the current benchmark, then invokes
   `scripts/check-bench-regression.sh` to detect regressions.

If the baseline artifact is unavailable (first run, no `gh` CLI, no previous
successful workflow), the script exits 0 and the regression check is skipped.
Set `GEEZIPX_BENCH_REQUIRE_COMPARISON=1` to make a missing baseline a hard error.

### Environment variables (all optional)

| Variable | Default | Description |
|---|---|---|
| `GEEZIPX_BENCH_BASELINE_ARTIFACT` | `criterion-report` | Artifact name to download |
| `GEEZIPX_BENCH_BASELINE_WORKFLOW` | `bench.yml` | Workflow filename |
| `GEEZIPX_BENCH_BASELINE_BRANCH` | `main` | Branch to fetch baseline from |
| `GEEZIPX_BENCH_REQUIRE_COMPARISON` | `0` | Fail if baseline is missing |
| `GH_TOKEN` | — | GitHub token for API auth |

```bash
# Example: point at a custom artifact
GEEZIPX_BENCH_BASELINE_ARTIFACT=my-criterion-report \
  bash scripts/setup-bench-baseline.sh
```
