#!/usr/bin/env bash
set -euo pipefail

# Check Criterion benchmark comparison results for regressions.
#
# Criterion writes relative comparison data to:
#   target/criterion/<group>/<bench>/change/estimates.json
#
# The `mean.point_estimate` value is a relative change ratio.  For example:
#   0.10  => 10% slower (regression)
#  -0.05  => 5% faster
#
# Environment variables:
#   GEEZIPX_BENCH_CRITERION_DIR             Criterion output dir (default: target/criterion)
#   GEEZIPX_BENCH_MAX_REGRESSION_PERCENT   Max allowed mean regression percent (default: 10)
#   GEEZIPX_BENCH_REQUIRE_COMPARISON        If true/1, fail when comparison data is missing

criterion_dir="${GEEZIPX_BENCH_CRITERION_DIR:-target/criterion}"
max_regression_percent="${GEEZIPX_BENCH_MAX_REGRESSION_PERCENT:-10}"
require_comparison="${GEEZIPX_BENCH_REQUIRE_COMPARISON:-0}"

python3 - "$criterion_dir" "$max_regression_percent" "$require_comparison" <<'PY'
import json
import sys
from pathlib import Path

criterion_dir = Path(sys.argv[1])
try:
    max_regression = float(sys.argv[2]) / 100.0
except ValueError:
    print(
        f"error: GEEZIPX_BENCH_MAX_REGRESSION_PERCENT must be a number, got {sys.argv[2]!r}",
        file=sys.stderr,
    )
    sys.exit(2)

require_comparison = sys.argv[3].strip().lower() in {"1", "true", "yes", "y"}

if not criterion_dir.exists():
    message = f"Criterion output directory not found: {criterion_dir}"
    if require_comparison:
        print(f"error: {message}", file=sys.stderr)
        sys.exit(1)
    print(f"bench-regression: {message}; skipping threshold check")
    sys.exit(0)

estimate_files = sorted(criterion_dir.glob("**/change/estimates.json"))
if not estimate_files:
    message = (
        "no Criterion comparison files found; run benchmarks with an existing "
        "Criterion baseline/cache to produce */change/estimates.json"
    )
    if require_comparison:
        print(f"error: {message}", file=sys.stderr)
        sys.exit(1)
    print(f"bench-regression: {message}; skipping threshold check")
    sys.exit(0)

failures = []
checked = 0
for path in estimate_files:
    rel = path.relative_to(criterion_dir)
    bench_name = str(rel.parent.parent) if rel.parent.name == "change" else str(rel)

    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        mean_change = float(data["mean"]["point_estimate"])
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
        failures.append((bench_name, f"invalid estimates file: {exc}"))
        continue

    checked += 1
    percent = mean_change * 100.0
    status = "regression" if mean_change > 0 else "improvement"
    print(f"bench-regression: {bench_name}: {percent:+.2f}% mean {status}")

    if mean_change > max_regression:
        failures.append(
            (
                bench_name,
                f"{percent:+.2f}% exceeds allowed +{max_regression * 100.0:.2f}%",
            )
        )

if failures:
    print("\nbenchmark regression threshold failed:", file=sys.stderr)
    for bench_name, reason in failures:
        print(f"  - {bench_name}: {reason}", file=sys.stderr)
    sys.exit(1)

print(
    f"bench-regression: checked {checked} benchmark comparison(s); "
    f"allowed mean regression <= +{max_regression * 100.0:.2f}%"
)
PY
