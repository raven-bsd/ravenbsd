#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C64j libc symbol audit: compare libc.so.1's exported dynamic symbols with the symbols the
# seven on-target C cases reference. The required inventory is in
# tests/libc/symbol-audit/required.tsv; the current snapshot is expected.tsv. Host-only: no QEMU.
#
# C64l: `cargo xtask libc-audit` forwards here; the C74 gate row runs that command.
#
# usage: scripts/libc-symbol-audit.sh [--rebuild] [--exports FILE] [--required FILE]
#                                    [--out FILE] [--expected FILE] [--self-test]
#
# Exit: 0 all required symbols are exported; 1 one or more are missing (the matrix is still
# written); 2 usage, I/O or a missing host tool. --self-test exercises the comparison with fake
# symbol tables and exits 0 when the fixtures match.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"

TAB=$(printf '\t')
EXPORTS=build/x86_64/libc.so.1
REQUIRED=tests/libc/symbol-audit/required.tsv
EXPECTED=tests/libc/symbol-audit/expected.tsv
OUT=build/libc-symbol-audit/matrix.tsv
SELF=tests/libc/symbol-audit/self-test
REBUILD=0
SELF_TEST=0

usage() {
    printf 'usage: %s [--rebuild] [--exports FILE] [--required FILE] [--out FILE] [--expected FILE] [--self-test]\n' "$0" >&2
    exit 2
}

die() {
    printf 'libc-symbol-audit: error: %s\n' "$*" >&2
    exit 2
}

have() {
    command -v "$1" >/dev/null 2>&1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --rebuild) REBUILD=1 ;;
        --exports)
            [ "$#" -ge 2 ] || usage
            EXPORTS=$2
            shift
            ;;
        --required)
            [ "$#" -ge 2 ] || usage
            REQUIRED=$2
            shift
            ;;
        --out)
            [ "$#" -ge 2 ] || usage
            OUT=$2
            shift
            ;;
        --expected)
            [ "$#" -ge 2 ] || usage
            EXPECTED=$2
            shift
            ;;
        --self-test) SELF_TEST=1 ;;
        -h|--help) usage ;;
        *) die "unknown argument $1" ;;
    esac
    shift
done

tmp=${TMPDIR:-/tmp}/libc-symbol-audit.$$
mkdir "$tmp" || die "cannot create $tmp"
trap 'rm -rf "$tmp"' 0 1 2 15

extract_exports() {
    file=$1
    if have nm; then
        nm -D --defined-only "$file" | awk '{print $NF}' | sed 's/@.*//' | LC_ALL=C sort -u
    elif have llvm-nm; then
        llvm-nm -D --defined-only "$file" | awk '{print $NF}' | sed 's/@.*//' | LC_ALL=C sort -u
    else
        die "nm/llvm-nm is not installed"
    fi
}

# run_audit EXPORTS REQUIRED OUT: write the deterministic matrix and return 1 for a missing symbol.
run_audit() {
    exports_file=$1
    required_file=$2
    out_file=$3
    [ -f "$exports_file" ] || return 2
    [ -f "$required_file" ] || return 2
    : > "$out_file"
    required_count=0
    present_count=0
    missing_count=0
    while IFS="$TAB" read -r case_name symbol owner note; do
        case "$case_name" in
            ''|'#'*) continue ;;
            case) continue ;;
        esac
        [ -n "$symbol" ] || continue
        required_count=$((required_count + 1))
        if grep -Fqx "$symbol" "$exports_file"; then
            status=PRESENT
            present_count=$((present_count + 1))
        else
            status=MISSING
            missing_count=$((missing_count + 1))
        fi
        printf '%s\t%s\t%s\t%s\t%s\n' "$case_name" "$symbol" "$status" "$owner" "$note" >> "$out_file"
    done < "$required_file"
    LC_ALL=C sort -t "$TAB" -k1,1 -k2,2 "$out_file" -o "$out_file"
    printf 'libc-symbol-audit: required=%d present=%d missing=%d\n' \
        "$required_count" "$present_count" "$missing_count"
    [ "$missing_count" -eq 0 ]
}

compare_matrix() {
    actual=$1
    expected=$2
    [ -f "$expected" ] || die "missing expected matrix $expected"
    if ! cmp -s "$actual" "$expected"; then
        printf 'libc-symbol-audit: matrix differs from %s\n' "$expected" >&2
        diff -u "$expected" "$actual" >&2 || true
        return 1
    fi
    return 0
}

self_test() {
    base=$SELF
    [ -d "$base" ] || die "missing self-test directory $base"

    if run_audit "$base/exports-present.txt" "$base/required.tsv" "$tmp/present.tsv"; then
        present_status=0
    else
        present_status=$?
    fi
    [ "$present_status" -eq 0 ] || die "present fixture exited $present_status"
    compare_matrix "$tmp/present.tsv" "$base/expected-present.tsv" \
        || die "present fixture matrix mismatch"

    if run_audit "$base/exports-missing.txt" "$base/required.tsv" "$tmp/missing.tsv"; then
        missing_status=0
    else
        missing_status=$?
    fi
    [ "$missing_status" -eq 1 ] || die "missing fixture exited $missing_status, expected 1"
    compare_matrix "$tmp/missing.tsv" "$base/expected-missing.tsv" \
        || die "missing fixture matrix mismatch"

    if run_audit "$base/exports-present.txt" "$base/required.tsv" "$tmp/present2.tsv"; then
        second_status=0
    else
        second_status=$?
    fi
    [ "$second_status" -eq 0 ] || die "second present run exited $second_status"
    cmp -s "$tmp/present.tsv" "$tmp/present2.tsv" \
        || die "matrix is not deterministic across runs"

    printf 'libc-symbol-audit: self-test ok\n'
}

if [ "$SELF_TEST" -eq 1 ]; then
    self_test
    exit 0
fi

if [ "$REBUILD" -eq 1 ] || [ ! -f "$EXPORTS" ]; then
    cargo xtask libc-pack --arch x86_64
    cargo xtask libc-link --arch x86_64
    # libc-pack writes the transient mkmap form of Symbols.list; restore the canonical generated
    # form so the audit leaves the tracked tree unchanged.
    cargo xtask libc-abi gen >/dev/null
fi
[ -f "$EXPORTS" ] || die "$EXPORTS is missing; run with --rebuild"
[ -f "$REQUIRED" ] || die "$REQUIRED is missing"

extract_exports "$EXPORTS" > "$tmp/exports"
mkdir -p "$(dirname "$OUT")"
if run_audit "$tmp/exports" "$REQUIRED" "$OUT"; then
    status=0
else
    status=$?
fi
if [ -f "$EXPECTED" ]; then
    if ! compare_matrix "$OUT" "$EXPECTED"; then
        status=1
    fi
fi
printf 'libc-symbol-audit: matrix %s\n' "$OUT"
exit "$status"
