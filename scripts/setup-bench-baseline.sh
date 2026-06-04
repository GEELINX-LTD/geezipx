#!/usr/bin/env bash
# ============================================================================
# setup-bench-baseline.sh — Download Criterion baseline artifact from GitHub
#
# Downloads the Criterion benchmark artifact from the last successful
# bench.yml run on the main branch, so that subsequent `cargo bench`
# produces comparison data for regression detection.
#
# Usage:
#   bash scripts/setup-bench-baseline.sh
#
# Environment variables (all optional):
#   GEEZIPX_BENCH_BASELINE_ARTIFACT   Artifact name (default: criterion-report)
#   GEEZIPX_BENCH_BASELINE_WORKFLOW   Workflow filename (default: bench.yml)
#   GEEZIPX_BENCH_BASELINE_BRANCH     Branch to fetch baseline from (default: main)
#   GH_TOKEN                          GitHub token for `gh` authentication
#
# Graceful degradation: exits 0 when `gh` is unavailable, no successful
# workflow run exists, or the artifact cannot be downloaded. Set
# GEEZIPX_BENCH_REQUIRE_COMPARISON=1 to make missing baseline a hard error.
# ============================================================================

set -euo pipefail

artifact="${GEEZIPX_BENCH_BASELINE_ARTIFACT:-criterion-report}"
workflow="${GEEZIPX_BENCH_BASELINE_WORKFLOW:-bench.yml}"
branch="${GEEZIPX_BENCH_BASELINE_BRANCH:-main}"
require="${GEEZIPX_BENCH_REQUIRE_COMPARISON:-0}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
criterion_dir="${GEEZIPX_BENCH_CRITERION_DIR:-$ROOT/target/criterion}"
mkdir -p "$criterion_dir"

# --------------------------------------------------------------------------
# Check prerequisites
# --------------------------------------------------------------------------
if ! command -v gh &>/dev/null; then
    if [ "$require" = "1" ]; then
        echo "error: 'gh' CLI not found; cannot download benchmark baseline" >&2
        exit 1
    fi
    echo "setup-bench-baseline: 'gh' CLI not found; skipping baseline download"
    exit 0
fi

# Verify we are in (or near) a GitHub repository
if ! git -C "$ROOT" rev-parse --is-inside-work-tree &>/dev/null; then
    if [ "$require" = "1" ]; then
        echo "error: not inside a Git repository" >&2
        exit 1
    fi
    echo "setup-bench-baseline: not inside a Git repository; skipping baseline download"
    exit 0
fi

# Check that we have a remote origin pointing to GitHub
remote_url="$(git -C "$ROOT" config --get remote.origin.url 2>/dev/null || true)"
if [ -z "$remote_url" ] || ! echo "$remote_url" | grep -qiE 'github\.com'; then
    if [ "$require" = "1" ]; then
        echo "error: remote origin is not GitHub; cannot download baseline" >&2
        exit 1
    fi
    echo "setup-bench-baseline: remote origin is not GitHub; skipping baseline download"
    exit 0
fi

# Extract owner/repo from remote URL for explicit --repo flag
# Handles both HTTPS (https://github.com/OWNER/REPO.git) and SSH (git@github.com:OWNER/REPO.git)
owner_repo="$(echo "$remote_url" | sed -nE 's#.*github\.com[/:]([^/]+/[^/.]+)(\.git)?$#\1#p')"

# --------------------------------------------------------------------------
# Find the latest successful workflow run
# --------------------------------------------------------------------------
echo "setup-bench-baseline: looking for latest successful '$workflow' run on '$branch'..."

run_id="$(
    gh run list \
        --workflow "$workflow" \
        --branch "$branch" \
        --repo "$owner_repo" \
        --event push \
        --status success \
        --limit 1 \
        --json databaseId,conclusion \
        --jq '.[0].databaseId' \
        2>/dev/null || true
)"

if [ -z "$run_id" ] || [ "$run_id" = "null" ]; then
    if [ "$require" = "1" ]; then
        echo "error: no successful '$workflow' run found on branch '$branch'" >&2
        exit 1
    fi
    echo "setup-bench-baseline: no successful '$workflow' run found on '$branch'; skipping"
    exit 0
fi

echo "setup-bench-baseline: found run #$run_id"

# --------------------------------------------------------------------------
# Download the artifact
# --------------------------------------------------------------------------
echo "setup-bench-baseline: downloading artifact '$artifact' from run #$run_id ..."

if ! gh run download "$run_id" --name "$artifact" --dir "$criterion_dir" --repo "$owner_repo" 2>/dev/null; then
    if [ "$require" = "1" ]; then
        echo "error: failed to download artifact '$artifact' from run #$run_id" >&2
        exit 1
    fi
    echo "setup-bench-baseline: failed to download artifact '$artifact'; skipping"
    exit 0
fi

echo "setup-bench-baseline: baseline artifact downloaded to $criterion_dir"
exit 0
