#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: gen-manifest.sh <source-tree> <manifest>
#
# Writes one line per path in the source tree:
#   path|type|mode|uid|gid|size|sha256|link-target|nlink
# The manifest is computed from the source tree, never from an image.
# Directory sizes are '-' because they depend on the host filesystem.
set -eu

tree=${1:?usage: gen-manifest.sh <source-tree> <manifest>}
out=${2:?usage: gen-manifest.sh <source-tree> <manifest>}

if [ ! -d "$tree" ]; then
    echo "gen-manifest.sh: not a directory: $tree" >&2
    exit 1
fi

mode4() {
    case $1 in
        ???) printf '0%s' "$1" ;;
        *)   printf '%s' "$1" ;;
    esac
}

emit() { # emit <absolute-path> <relative-path>
    p=$1
    rel=$2
    ftype=$(stat -c '%F' "$p")
    mode=$(mode4 "$(stat -c '%a' "$p")")
    uid=$(stat -c '%u' "$p")
    gid=$(stat -c '%g' "$p")
    nlink=$(stat -c '%h' "$p")
    size=$(stat -c '%s' "$p")
    sha=-
    link=-

    case $ftype in
        'regular file'|'regular empty file')
            type=file
            sha=$(sha256sum "$p" | cut -d' ' -f1)
            ;;
        'directory')
            type=dir
            size=-
            ;;
        'symbolic link')
            type=symlink
            link=$(readlink "$p")
            # makefs/FFS normalises symlink modes to 0755 (walk.c: "force
            # them to 0755"); Linux lstat reports 0777.  Record the FFS rule.
            mode=0755
            ;;
        'character special file')
            type=char
            size=0
            ;;
        'block special file')
            type=block
            size=0
            ;;
        'fifo')
            type=fifo
            size=0
            ;;
        'socket')
            type=socket
            size=0
            ;;
        *)
            echo "gen-manifest.sh: unsupported file type '$ftype' at $p" >&2
            exit 1
            ;;
    esac

    printf '%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \
        "$rel" "$type" "$mode" "$uid" "$gid" "$size" "$sha" "$link" "$nlink"
}

{
    emit "$tree" /
    find "$tree" -mindepth 1 -print | LC_ALL=C sort | while IFS= read -r p; do
        emit "$p" "${p#"$tree"}"
    done
} > "$out"
