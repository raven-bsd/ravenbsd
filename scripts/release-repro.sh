#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K146: release image A/B reproducibility check.
#
# Builds the x86_64 release image twice from a clean worktree at the same commit and compares
# size and SHA-256. Both builds use the product's `cargo xtask image --profile release --output`
# path; one build is never copied or reused for the other. SOURCE_DATE_EPOCH defaults to the HEAD
# commit time (the standard reproducible-build input) and is recorded.
#
# Outputs (not committed): build/release/repro/
#   a.img, b.img            the two release images
#   build-a.log, build-b.log
#   summary.txt             commit/epoch/host/command/build table/equality
#   sha256.txt              both hashes
#   cmp.txt, cmp-first-bytes.txt, a-range.txt, b-range.txt   only on a mismatch
#
# Exit: 0 the two images are byte-identical; 1 mismatch/build failure/dirty tree; 2 bad invocation.

set -u
LC_ALL=C
export LC_ALL

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT" || exit 1

OUT=${1:-build/release/repro}
mkdir -p "$OUT" || exit 2

commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)
branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
if [ -z "${SOURCE_DATE_EPOCH:-}" ]; then
    SOURCE_DATE_EPOCH=$(git log -1 --format=%ct 2>/dev/null || echo 0)
fi
export SOURCE_DATE_EPOCH

command_a="cargo xtask image --arch x86_64 --profile release --output $OUT/a.img"
command_b="cargo xtask image --arch x86_64 --profile release --output $OUT/b.img"

summary=$OUT/summary.txt
{
    printf 'commit=%s\n' "$commit"
    printf 'branch=%s\n' "$branch"
    printf 'source_date_epoch=%s\n' "$SOURCE_DATE_EPOCH"
    printf 'nproc=%s\n' "$(nproc 2>/dev/null || echo unknown)"
    printf 'host_load=%s\n' "$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo unknown)"
    qemu=$(pgrep -c -f '^qemu-system-' 2>/dev/null || true)
    printf 'host_qemu=%s\n' "${qemu:-0}"
    printf 'command_a=%s\n' "$command_a"
    printf 'command_b=%s\n' "$command_b"
} > "$summary"

dirty=$(git status --porcelain 2>/dev/null)
if [ -n "$dirty" ]; then
    printf 'release-repro: FAIL (worktree is not clean)\n'
    printf 'dirty_entries:\n%s\n' "$dirty" >> "$summary"
    printf 'equal=no\nreason=dirty-worktree\n' >> "$summary"
    exit 1
fi

build() {
    label=$1
    image=$2
    log=$3
    start_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    start_ns=$(date +%s%N)
    # shellcheck disable=SC2086
    cargo xtask image --arch x86_64 --profile release --output "$image" > "$log" 2>&1
    rc=$?
    end_ns=$(date +%s%N)
    end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
    size=$(stat -c %s "$image" 2>/dev/null || echo 0)
    hash=$(sha256sum "$image" 2>/dev/null | cut -d' ' -f1)
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$label" "$start_utc" "$end_utc" "$elapsed_ms" "$size" "${hash:-missing}" "$rc" \
        >> "$OUT/builds.tsv"
    printf 'build_%s: start=%s end=%s elapsed_ms=%s rc=%s size=%s sha256=%s\n' \
        "$label" "$start_utc" "$end_utc" "$elapsed_ms" "$rc" "$size" "${hash:-missing}" \
        >> "$summary"
}

printf 'build\tstart_utc\tend_utc\telapsed_ms\tsize\tsha256\trc\n' > "$OUT/builds.tsv"
build a "$OUT/a.img" "$OUT/build-a.log"
build b "$OUT/b.img" "$OUT/build-b.log"

rc_a=$(awk -F'\t' '$1 == "a" { print $7 }' "$OUT/builds.tsv")
rc_b=$(awk -F'\t' '$1 == "b" { print $7 }' "$OUT/builds.tsv")
size_a=$(awk -F'\t' '$1 == "a" { print $5 }' "$OUT/builds.tsv")
size_b=$(awk -F'\t' '$1 == "b" { print $5 }' "$OUT/builds.tsv")
hash_a=$(awk -F'\t' '$1 == "a" { print $6 }' "$OUT/builds.tsv")
hash_b=$(awk -F'\t' '$1 == "b" { print $6 }' "$OUT/builds.tsv")

sha256sum "$OUT/a.img" "$OUT/b.img" > "$OUT/sha256.txt" 2>/dev/null || true

if [ "$rc_a" != "0" ] || [ "$rc_b" != "0" ]; then
    printf 'release-repro: FAIL (build failed: a=%s b=%s)\n' "$rc_a" "$rc_b"
    printf 'equal=no\nreason=build-failed\n' >> "$summary"
    tail -n 10 "$OUT/build-a.log" 2>/dev/null || true
    tail -n 10 "$OUT/build-b.log" 2>/dev/null || true
    exit 1
fi

if [ "$size_a" = "$size_b" ] && [ "$hash_a" = "$hash_b" ] && cmp -s "$OUT/a.img" "$OUT/b.img"; then
    printf 'equal=yes\n' >> "$summary"
    printf 'release-repro: PASS commit=%s size=%s sha256=%s elapsed_a_b_ms=%s/%s\n' \
        "$commit" "$size_a" "$hash_a" \
        "$(awk -F'\t' '$1 == "a" { print $4 }' "$OUT/builds.tsv")" \
        "$(awk -F'\t' '$1 == "b" { print $4 }' "$OUT/builds.tsv")"
    exit 0
fi

printf 'equal=no\n' >> "$summary"
cmp "$OUT/a.img" "$OUT/b.img" > "$OUT/cmp.txt" 2>&1 || true
cmp -l "$OUT/a.img" "$OUT/b.img" 2>/dev/null | head -n 5 > "$OUT/cmp-first-bytes.txt" || true
first=$(awk 'NR == 1 { print $1 }' "$OUT/cmp-first-bytes.txt" 2>/dev/null)
if [ -n "$first" ]; then
    offset=$((first - 1))
    start=$((offset - 64))
    [ "$start" -lt 0 ] && start=0
    od -An -tx1 -j "$start" -N 160 "$OUT/a.img" > "$OUT/a-range.txt" 2>/dev/null || true
    od -An -tx1 -j "$start" -N 160 "$OUT/b.img" > "$OUT/b-range.txt" 2>/dev/null || true
    printf 'first_diff_offset=%s\n' "$offset" >> "$summary"
fi
printf 'release-repro: FAIL commit=%s size_a=%s size_b=%s sha256_a=%s sha256_b=%s\n' \
    "$commit" "$size_a" "$size_b" "$hash_a" "$hash_b"
printf 'release-repro: first differing bytes (cmp -l):\n'
cat "$OUT/cmp-first-bytes.txt" 2>/dev/null || true
printf 'release-repro: cmp: '
cat "$OUT/cmp.txt" 2>/dev/null || true
printf 'release-repro: evidence in %s\n' "$OUT"
exit 1
