# Scripts

## verify-release-artifacts.sh

Verifies downloaded release artifacts before uploading or publishing them:

```bash
bash scripts/verify-release-artifacts.sh artifacts
```

Requires nonempty Linux/macOS/Windows CLI archives and matching original
`.sha256` files, plus nonempty AppImage, deb, DMG and NSIS (`*-setup.exe`)
GUI bundles. Missing files, empty files and CLI checksum mismatches fail the
command. After validation, it writes individual checksums and `SHA256SUMS`
for all bundles. When `GITHUB_STEP_SUMMARY` is set, it appends a table with
filenames, sizes and hashes.

The Release workflow runs this script and uploads the verified files as
`geezipx-release-artifacts`, including during a manual `dry_run: true` build.

## check-format-candidate.py

Runs 35 checks against an already-built native CLI, plus a native SFX check
with `--sfx`, using Python 3.12 and only its standard library. It uses offline
fixtures with recorded provenance and
checks real subprocess exit codes, JSON listing/verification, extracted content,
Unicode filesystem paths, corrupt-stream output protection and wrong-password
handling. With `--sfx`, it creates and executes a native self-extracting archive.

```bash
cargo build --locked --release -p geezipx --all-features
python3 scripts/check-format-candidate.py --binary target/release/geezipx --sfx --report candidate-report.json
```

The build must embed a current host SFX stub (as done in CI). On Windows the
binary path ends in `geezipx.exe`. The report records source SHA, native OS and
architecture, feature arguments, CLI version, executable SHA-256, checks and
extracted file hashes. CI uploads separate reports with each platform's CLI
artifact; the first error fails the step and leaves a partial failure report.

Coverage includes ZIP/XZ method 95, 7z, TAR/TGZ, LZH/lh7, ISO, GZIP/XZ, real
EGG/PMA/Unix Compress/Lzip/ISZ/AES Crypt fixtures, TLZ, cooked IMG/BIN views,
synthetic ALZ/CFB regressions and native SFX. Synthetic containers are not
independent interoperability evidence; MSI database extraction and unsupported
format variants are outside this check. These small cases are correctness
acceptance, not a performance benchmark or exhaustive corruption test.

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
