#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# `make prune`: remove build cache that Cargo does not need for the current artifacts.
# Dry-run by default. Never truncates Cargo artifacts; incremental caches, docs, package/tmp,
# archive-export scratch and old build images/logs are regenerable.
#
# Usage:
#   sh scripts/prune.sh [--apply] [--days N] [--deps] [--no-build]
#
# The script refuses to touch anything while cargo/rustc/xtask is running. It skips build
# images and logs while QEMU is running, because a guest may still be using one.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

apply=0
days=14
deps=0
build=1

while [ "$#" -gt 0 ]; do
    case "$1" in
        --apply) apply=1 ;;
        --days) shift; days=${1:?--days needs a number} ;;
        --deps) deps=1 ;;
        --no-build) build=0 ;;
        -h|--help)
            echo "usage: sh scripts/prune.sh [--apply] [--days N] [--deps] [--no-build]"
            exit 0
            ;;
        *)
            echo "prune: unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

if ! [ "$days" -gt 0 ] 2>/dev/null; then
    echo "prune: --days must be a positive integer" >&2
    exit 2
fi

busy=0
for process in cargo rustc xtask; do
    if pgrep -x "$process" >/dev/null 2>&1; then
        echo "prune: $process is running" >&2
        busy=1
    fi
done
if [ "$busy" -eq 1 ] && [ "$apply" -eq 1 ]; then
    echo "prune: refusing to delete while a build is running" >&2
    exit 2
fi
if [ "$busy" -eq 1 ]; then
    echo "prune: build activity detected; dry-run only, apply is refused" >&2
fi

qemu=0
if pgrep -x qemu-system-x86_64 >/dev/null 2>&1 ||
    pgrep -x qemu-system-aarch64 >/dev/null 2>&1; then
    qemu=1
fi

remove_tree() {
    [ -e "$1" ] || return 0
    if [ "$apply" -eq 1 ]; then
        rm -rf "$1"
        printf 'removed: %s\n' "$1"
    else
        printf 'would remove: %s\n' "$1"
    fi
}

remove_file() {
    [ -e "$1" ] || return 0
    if [ "$apply" -eq 1 ]; then
        rm -f "$1"
        printf 'removed: %s\n' "$1"
    else
        printf 'would remove: %s\n' "$1"
    fi
}

echo "RavenBSD build prune ($([ "$apply" -eq 1 ] && echo apply || echo dry-run))"
echo "days=$days deps=$deps build=$build qemu=$qemu"
echo

# Incremental compilation data is a cache; compiled artifacts stay in deps/.
find target -type d -name incremental -prune -print 2>/dev/null |
    while IFS= read -r dir; do
        remove_tree "$dir"
    done

# xtask archive-export scratch is not referenced by Cargo.
for dir in target/archive-exports-*; do
    [ -d "$dir" ] || continue
    remove_tree "$dir"
done

for dir in target/doc target/package target/tmp; do
    remove_tree "$dir"
done

if [ "$build" -eq 1 ]; then
    if [ "$qemu" -eq 1 ] && [ "$apply" -eq 1 ]; then
        echo "build images/logs: skipped, QEMU is running"
    else
        if [ "$qemu" -eq 1 ]; then
            echo "build images/logs: QEMU is running; apply will skip this category"
        fi
        for tree in build/x86_64 build/aarch64 build/qtest build/fixtures \
            build/pi4-release build/soak build/posix-utility; do
            [ -d "$tree" ] || continue
            find "$tree" -type f \
                \( -name '*.img' -o -name '*.qcow2' -o -name '*.iso' -o -name '*.log' \) \
                -mtime +"$days" -print 2>/dev/null | while IFS= read -r file; do
                remove_file "$file"
            done
        done
    fi
fi

if [ "$deps" -eq 1 ]; then
    if command -v cargo-sweep >/dev/null 2>&1; then
        if [ "$apply" -eq 1 ]; then
            cargo sweep --time "$days"
        else
            echo "would run: cargo sweep --time $days"
        fi
    else
        echo "deps: cargo-sweep not installed; skipping"
    fi
fi

echo
if [ "$apply" -eq 0 ]; then
    echo "dry-run only; set PRUNE_APPLY=1 (or pass --apply) to delete"
fi
