#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: gen-tree.sh
#
# Creates the deterministic UFS test tree at $UFS_ORACLE_DIR/tree.
# The tree carries uid 1000, gid 100, fixed timestamps and a character
# device and FIFO.  Those need root, so the script re-executes itself under
# fakeroot when available (the host has it); otherwise it refuses to run
# rather than silently producing different metadata.
set -eu

: "${UFS_ORACLE_DIR:=/tmp/opencode/ufs-oracle}"
STAMP=1767225600        # 2026-01-01T00:00:00Z
TREE=$UFS_ORACLE_DIR/tree

if [ -z "${FAKEROOTKEY:-}" ] && [ "$(id -u)" -ne 0 ]; then
    if command -v fakeroot >/dev/null 2>&1; then
        exec fakeroot -- sh "$0" "$@"
    fi
    echo "gen-tree.sh: needs fakeroot or root (uid/gid 1000:100 and device nodes)" >&2
    exit 1
fi

rm -rf "$TREE"
mkdir -p "$TREE"

# 64-byte lines; each line names its 4 KiB logical block, so the content is
# text and compresses well.  All sizes below are multiples of 64.
gen_text() { # gen_text <path> <bytes>
    awk -v size="$2" 'BEGIN {
        pad = "......................................"
        n = size / 64
        for (i = 0; i < n; i++)
            printf "block %06d line %06d %s\n", int(i / 64), i % 64, pad
    }' > "$1"
    # Guard against an unexpected line length.
    [ "$(wc -c < "$1")" -eq "$2" ] || {
        echo "gen-tree.sh: internal error generating $1" >&2
        exit 1
    }
}

: > "$TREE/empty"
printf '0123456789' > "$TREE/ten.txt"
gen_text "$TREE/frag1k.bin" 1024
gen_text "$TREE/frag3k.bin" 3072
gen_text "$TREE/frag7k.bin" 7168
gen_text "$TREE/block1.bin" 4096
gen_text "$TREE/blocks12.bin" $((12 * 4096))
gen_text "$TREE/blocks13.bin" $((13 * 4096))
gen_text "$TREE/blocks600.bin" $((600 * 4096))
gen_text "$TREE/big3m.bin" $((3 * 1024 * 1024))

printf 'hard link content\n' > "$TREE/hardlink-a.txt"
ln "$TREE/hardlink-a.txt" "$TREE/hardlink-b.txt"

ln -s fast-target "$TREE/fastlink"
slow_target=$(awk 'BEGIN { for (i = 0; i < 200; i++) printf "x" }')
ln -s "$slow_target" "$TREE/slowlink"

mkdir "$TREE/emptydir"

mkdir "$TREE/dir1"
printf 'one\n' > "$TREE/dir1/one.txt"

mkdir "$TREE/dir100"
i=0
while [ "$i" -lt 100 ]; do
    printf 'entry %03d\n' "$i" > "$TREE/dir100/entry-$(printf '%03d' "$i")"
    i=$((i + 1))
done

mkdir "$TREE/dir1500"
i=0
while [ "$i" -lt 1500 ]; do
    printf 'entry %04d\n' "$i" > "$TREE/dir1500/entry-$(printf '%04d' "$i")"
    i=$((i + 1))
done

longname=$(awk 'BEGIN { for (i = 0; i < 251; i++) printf "n"; printf ".txt" }')
printf 'long name\n' > "$TREE/$longname"

path=$TREE/deep
mkdir "$path"
i=1
while [ "$i" -le 12 ]; do
    path=$path/$(printf 'd%02d' "$i")
    mkdir "$path"
    i=$((i + 1))
done
printf 'leaf\n' > "$path/leaf.txt"

printf 'setuid content\n' > "$TREE/mode4755.bin"
printf 'private content\n' > "$TREE/mode600.txt"

mknod "$TREE/cdev" c 1 3
mkfifo "$TREE/fifo"

# Ownership first: chown clears setuid/setgid bits on Linux, so modes follow.
chown -R 1000:100 "$TREE"
find "$TREE" -type d -exec chmod 0755 {} +
find "$TREE" -type f -exec chmod 0644 {} +
chmod 4755 "$TREE/mode4755.bin"
chmod 0600 "$TREE/mode600.txt"
chmod 0644 "$TREE/cdev" "$TREE/fifo"

# Fixed timestamps.  makefs is also given -T 1767225600, so the image does
# not depend on these, but the tree itself is then reproducible too.
find "$TREE" -exec touch -h -d "@$STAMP" {} +

echo "gen-tree.sh: $TREE ($(find "$TREE" | wc -l) paths)"
