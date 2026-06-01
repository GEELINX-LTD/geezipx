#!/usr/bin/env bash
# ============================================================================
# check-interop.sh — GeeZipX Format Interoperability Smoke Tests
#
# Builds the GeeZipX binary and runs smoke tests against native system tools:
#   unzip, tar, gzip.
#
# Usage:
#   bash scripts/check-interop.sh
#   GEEZIPX_INTEROP_STRESS=1 bash scripts/check-interop.sh   # heavier smoke
# ============================================================================

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug/geezipx"
TMPDIR="$(mktemp -d)"

PASS=0
SKIP=0
FAIL=0

cleanup() {
    rm -rf "$TMPDIR"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
need_tool() {
    if ! command -v "$1" &>/dev/null; then
        echo "  SKIP  $2  ($1 not found)"
        SKIP=$((SKIP + 1))
        return 1
    fi
    return 0
}

pass() {
    echo "  PASS  $1"
    PASS=$((PASS + 1))
}

fail() {
    echo "  FAIL  $1"
    FAIL=$((FAIL + 1))
}

header() {
    echo ""
    echo "=== $1 ==="
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
echo ":: Building GeeZipX ..."
cargo build -p geezipx -q 2>/dev/null
echo ":: Binary: $BIN"
echo ":: Temp dir: $TMPDIR"
echo ""

cd "$TMPDIR"

# ---------------------------------------------------------------------------
# 1. ZIP -> unzip -t
# ---------------------------------------------------------------------------
header "GeeZipX ZIP validates with native unzip"
if need_tool unzip ZIP; then
    echo "Hello from GeeZipX ZIP" > hello.txt
    "$BIN" compress hello.txt -o test.zip --no-progress
    if unzip -t test.zip >/dev/null 2>&1; then
        pass "geezipx zip -> unzip -t"
    else
        fail "geezipx zip -> unzip -t"
    fi
fi

# ---------------------------------------------------------------------------
# 2. Native zip -> GeeZipX (if zip installed)
# ---------------------------------------------------------------------------
header "Native ZIP decompresses with GeeZipX"
if need_tool zip "native-zip"; then
    echo "Native zip via GeeZipX" > native.txt
    zip -j native.zip native.txt >/dev/null
    # List
    if "$BIN" list native.zip 2>/dev/null | grep -q native.txt; then
        pass "geezipx list native zip"
    else
        fail "geezipx list native zip"
    fi
    # Decompress
    mkdir -p out_zip
    if "$BIN" decompress native.zip -o out_zip --no-progress 2>/dev/null; then
        if diff -q native.txt out_zip/native.txt >/dev/null 2>&1; then
            pass "geezipx decompress native zip"
        else
            fail "geezipx decompress native zip (content mismatch)"
        fi
    else
        fail "geezipx decompress native zip"
    fi
fi

# ---------------------------------------------------------------------------
# 3. GeeZipX TAR -> tar tf
# ---------------------------------------------------------------------------
header "GeeZipX TAR lists with native tar"
if need_tool tar TAR; then
    mkdir -p tardir/inner
    echo "top" > tardir/top.txt
    echo "nested" > tardir/inner/deep.txt
    "$BIN" compress tardir -r -f tar -o out.tar --no-progress
    listing=$(tar tf out.tar 2>/dev/null)
    if echo "$listing" | grep -q 'top.txt'; then
        pass "geezipx tar -> tar tf contains top.txt"
    else
        fail "geezipx tar -> tar tf missing top.txt"
    fi
    if echo "$listing" | grep -qE 'deep.txt|inner/deep.txt'; then
        pass "geezipx tar -> tar tf contains deep.txt"
    else
        fail "geezipx tar -> tar tf missing deep.txt"
    fi
fi

# ---------------------------------------------------------------------------
# 4. Native tar -> GeeZipX decompress
# ---------------------------------------------------------------------------
header "Native TAR decompresses with GeeZipX"
if need_tool tar "native-tar-decompress"; then
    mkdir -p native_tar/nested
    echo "hello" > native_tar/hello.txt
    echo "world" > native_tar/nested/world.txt
    tar cf native.tar -C native_tar .
    mkdir -p out_tar
    if "$BIN" decompress native.tar -o out_tar --no-progress 2>/dev/null; then
        if [ -f out_tar/hello.txt ]; then
            pass "geezipx decompress native tar (hello.txt)"
        else
            fail "geezipx decompress native tar (hello.txt missing)"
        fi
        if [ -f out_tar/nested/world.txt ]; then
            pass "geezipx decompress native tar (nested/world.txt)"
        else
            fail "geezipx decompress native tar (nested/world.txt missing)"
        fi
    else
        fail "geezipx decompress native tar"
    fi
fi

# ---------------------------------------------------------------------------
# 5. GeeZipX TAR.GZ -> tar tzf
# ---------------------------------------------------------------------------
header "GeeZipX TAR.GZ lists with native tar"
if need_tool tar TARGZ; then
    mkdir -p tgzdir/sub
    echo "a" > tgzdir/a.txt
    echo "b" > tgzdir/sub/b.txt
    "$BIN" compress tgzdir -r -f tar.gz -o archive.tar.gz --no-progress
    listing=$(tar tzf archive.tar.gz 2>/dev/null)
    if echo "$listing" | grep -q 'a.txt'; then
        pass "geezipx tar.gz -> tar tzf contains a.txt"
    else
        fail "geezipx tar.gz -> tar tzf missing a.txt"
    fi
    if echo "$listing" | grep -qE 'b.txt|sub/b.txt'; then
        pass "geezipx tar.gz -> tar tzf contains b.txt"
    else
        fail "geezipx tar.gz -> tar tzf missing b.txt"
    fi
fi

# ---------------------------------------------------------------------------
# 6. Native tar.gz -> GeeZipX decompress
# ---------------------------------------------------------------------------
header "Native TAR.GZ decompresses with GeeZipX"
if need_tool tar "native-targz-decompress"; then
    mkdir -p native_tgz/sub
    echo "hello" > native_tgz/hello.txt
    echo "deep" > native_tgz/sub/deep.txt
    tar czf native.tar.gz -C native_tgz .
    mkdir -p out_tgz
    if "$BIN" decompress native.tar.gz -o out_tgz --no-progress 2>/dev/null; then
        if [ -f out_tgz/hello.txt ]; then
            pass "geezipx decompress native tar.gz (hello.txt)"
        else
            fail "geezipx decompress native tar.gz (hello.txt missing)"
        fi
        if [ -f out_tgz/sub/deep.txt ]; then
            pass "geezipx decompress native tar.gz (sub/deep.txt)"
        else
            fail "geezipx decompress native tar.gz (sub/deep.txt missing)"
        fi
    else
        fail "geezipx decompress native tar.gz"
    fi
fi

# ---------------------------------------------------------------------------
# 7. GeeZipX GZIP -> native gzip -dc
# ---------------------------------------------------------------------------
header "GeeZipX GZIP decompresses with native gzip"
if need_tool gzip GZIP; then
    echo "GeeZipX gzip data for native gzip -dc" > gz_data.txt
    "$BIN" compress gz_data.txt -f gz -o data.gz --no-progress
    decoded=$(gzip -dc data.gz 2>/dev/null)
    if [ "$decoded" = "$(cat gz_data.txt)" ]; then
        pass "geezipx gzip -> gzip -dc (content match)"
    else
        fail "geezipx gzip -> gzip -dc (content mismatch)"
    fi
fi

# ---------------------------------------------------------------------------
# 8. Native gzip -> GeeZipX --stdout
# ---------------------------------------------------------------------------
header "Native GZIP decompresses with GeeZipX --stdout"
if need_tool gzip "native-gzip-stdout"; then
    echo "Native gzip round-trip via GeeZipX --stdout" > native_gzip.txt
    gzip -c native_gzip.txt > native.gz
    decoded=$("$BIN" decompress native.gz --stdout --no-progress 2>/dev/null)
    expected=$(cat native_gzip.txt)
    if [ "$decoded" = "$expected" ]; then
        pass "native gzip -> geezipx --stdout (content match)"
    else
        fail "native gzip -> geezipx --stdout (content mismatch)"
    fi
fi

# ---------------------------------------------------------------------------
# Stress mode (optional)
# ---------------------------------------------------------------------------
if [ "${GEEZIPX_INTEROP_STRESS:-0}" = "1" ]; then
    header "STRESS: moderate-sized files"
    echo "Creating 256 MB file (dd zeros) ..."
    dd if=/dev/zero of=large.dat bs=1M count=256 2>/dev/null
    echo "Compressing with GeeZipX gzip ..."
    if "$BIN" compress large.dat -f gz -o large.gz --no-progress 2>/dev/null; then
        pass "stress: compress 256 MB file (gzip)"
        echo "Decompressing with native gzip ..."
        gzip -dc large.gz > large_decoded.dat 2>/dev/null
        if diff -q large.dat large_decoded.dat >/dev/null 2>&1; then
            pass "stress: native gzip decompress 256 MB (content match)"
        else
            fail "stress: native gzip decompress 256 MB (mismatch)"
        fi
    else
        fail "stress: compress 256 MB file"
    fi

    echo ""
    echo "Creating 1000 small files ..."
    mkdir -p stress_dir
    for i in $(seq 1 1000); do
        echo "file_$i" > "stress_dir/f_$i.txt"
    done
    echo "Compressing with GeeZipX tar.gz ..."
    if "$BIN" compress stress_dir -r -f tar.gz -o stress.tar.gz --no-progress 2>/dev/null; then
        pass "stress: compress 1000 files (tar.gz)"
        echo "Listing with tar tzf ..."
        count=$(tar tzf stress.tar.gz 2>/dev/null | grep -c 'f_')
        if [ "$count" -eq 1000 ]; then
            pass "stress: tar tzf lists 1000 files"
        else
            fail "stress: tar tzf lists only $count/1000 files"
        fi
    else
        fail "stress: compress 1000 files"
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "============ SUMMARY ============"
echo "  PASS: $PASS"
echo "  SKIP: $SKIP"
echo "  FAIL: $FAIL"
echo "================================="
if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
