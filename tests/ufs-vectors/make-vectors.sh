#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: make-vectors.sh
#
# Builds the four golden UFS images (UFS1/UFS2 x 4 KiB/8 KiB blocks) with the
# oracle makefs, gzips them deterministically and writes the expected
# manifests from the source tree.  Re-running must produce identical bytes.
#
# The tree carries uid 1000, gid 100 and a character device, so the script
# runs under fakeroot when it is not root.  Set UFS_ORACLE_DIR to move the
# scratch tree and oracle binaries (default /tmp/opencode/ufs-oracle).
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
: "${UFS_ORACLE_DIR:=/tmp/opencode/ufs-oracle}"
STAMP=1767225600

if [ -z "${FAKEROOTKEY:-}" ] && [ "$(id -u)" -ne 0 ]; then
    if command -v fakeroot >/dev/null 2>&1; then
        exec fakeroot -- sh "$0" "$@"
    fi
    echo "make-vectors.sh: needs fakeroot or root (uid/gid 1000:100 and device nodes)" >&2
    exit 1
fi

if [ ! -x "$UFS_ORACLE_DIR/makefs" ]; then
    echo "make-vectors.sh: oracle makefs missing, building it" >&2
    sh "$ROOT/tools/ufs-oracle/build-oracles.sh"
fi
if [ ! -x "$UFS_ORACLE_DIR/makefs" ]; then
    echo "make-vectors.sh: oracle makefs still missing" >&2
    exit 1
fi

sh "$ROOT/tests/ufs-vectors/gen-tree.sh"

GOLDEN=$ROOT/tests/ufs-vectors/golden
EXPECTED=$ROOT/tests/ufs-vectors/expected
mkdir -p "$GOLDEN" "$EXPECTED"

: > "$UFS_ORACLE_DIR/fsck-results.txt"
have_fsck=no
[ -x "$UFS_ORACLE_DIR/fsck_ffs" ] && have_fsck=yes

build_one() { # build_one <name> <version> <bsize> <fsize>
    name=$1
    version=$2
    bsize=$3
    fsize=$4
    img=$UFS_ORACLE_DIR/$name.img

    rm -f "$img"
    "$UFS_ORACLE_DIR/makefs" -B little -T "$STAMP" -t ffs \
        -o "version=$version,bsize=$bsize,fsize=$fsize,extattr=0" \
        -s 16m "$img" "$UFS_ORACLE_DIR/tree" \
        > "$UFS_ORACLE_DIR/$name.makefs.log" 2>&1

    actual=$(stat -c '%s' "$img")
    if [ "$actual" -ne 16777216 ]; then
        echo "make-vectors.sh: $name is $actual bytes, expected 16777216" >&2
        exit 1
    fi

    if [ "$have_fsck" = yes ]; then
        set +e
        "$UFS_ORACLE_DIR/fsck_ffs" -n -F "$img" \
            > "$UFS_ORACLE_DIR/$name.fsck.log" 2>&1
        rc=$?
        set -e
        printf '%s %d\n' "$name" "$rc" >> "$UFS_ORACLE_DIR/fsck-results.txt"
        echo "fsck_ffs -n -F $name.img: exit $rc"
    fi

    gzip -9 -n -c "$img" > "$GOLDEN/$name.img.gz"
    sh "$ROOT/tests/ufs-vectors/gen-manifest.sh" "$UFS_ORACLE_DIR/tree" "$EXPECTED/$name.manifest"
    python3 "$ROOT/tools/ufs-oracle/check-ufs.py" "$img" "$EXPECTED/$name.manifest"
    printf '%-12s image=%s gz=%s inodes=%s\n' "$name" "$actual" \
        "$(stat -c '%s' "$GOLDEN/$name.img.gz")" \
        "$(sed -n 's/.*bytes, \([0-9][0-9]*\) inodes.*/\1/p' "$UFS_ORACLE_DIR/$name.makefs.log")"
    rm -f "$img"
}

build_one ufs1-b4096 1 4096 512
build_one ufs1-b8192 1 8192 1024
build_one ufs2-b4096 2 4096 512
build_one ufs2-b8192 2 8192 1024

total=$(du -cb "$GOLDEN"/*.img.gz "$EXPECTED"/*.manifest | tail -n 1 | cut -f1)
echo "make-vectors.sh: committed payload $total bytes (budget 8388608)"
if [ "$total" -gt 8388608 ]; then
    echo "make-vectors.sh: over the 8 MiB budget" >&2
    exit 1
fi

if [ "$have_fsck" = yes ]; then
    echo "make-vectors.sh: fsck results:"
    cat "$UFS_ORACLE_DIR/fsck-results.txt"
else
    echo "make-vectors.sh: fsck_ffs not built; see tools/ufs-oracle/README.md"
fi

sha256sum "$GOLDEN"/*.img.gz "$EXPECTED"/*.manifest
