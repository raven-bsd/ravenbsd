#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Independent clean-clone release verification.
#
# Usage: sh scripts/release-verify.sh <scratch-dir>
#
# The scratch directory must be under /tmp. It is recreated and holds the
# clean clone, the two release images, the qtest transcript and this log.
# The verifier's own clone and product tree are read-only for this script.

set -eu

usage() {
    echo "usage: sh scripts/release-verify.sh <scratch-dir>" >&2
    echo "       <scratch-dir> must be under /tmp" >&2
}

case "${1:-}" in
    "" | -h | --help)
        usage
        [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ] || exit 2
        exit 0
        ;;
esac

[ "$#" -eq 1 ] || {
    echo "release-verify: exactly one scratch directory is required" >&2
    usage
    exit 2
}

dest=$1
case "$dest" in
    /tmp/*) ;;
    *)
        echo "release-verify: scratch dir must be under /tmp (got $dest)" >&2
        exit 2
        ;;
esac

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
verifier=$root

commit=$(git -C "$verifier" rev-parse origin/master)
commit_time=$(git -C "$verifier" show -s --format=%ct "$commit")

rm -rf "$dest"
mkdir -p "$dest"
clone=$dest/clone
log=$dest/transcript.log
: > "$log"

say() {
    echo "$*" | tee -a "$log"
}

run() {
    say "+ $*"
    if "$@" >>"$log" 2>&1; then
        return 0
    else
        code=$?
        say "release-verify: command failed (exit $code): $*"
        tail -n 40 "$log"
        return "$code"
    fi
}

say "release-verify: verifier=$verifier"
say "release-verify: commit=$commit time=$commit_time"
say "release-verify: scratch=$dest"

run git clone --shared --no-checkout "$verifier" "$clone"
run git -C "$clone" checkout --detach "$commit"

if ! run make -C "$clone" setup SETUP_ARGS=--check; then
    say "release-verify: setup --check failed; required host tools are missing:"
    grep 'MISSING' "$log" | tail -n 20 || true
    exit 3
fi

# The release gate is the clean-tree build proof: its qtest builds every
# prerequisite in the fresh clone. The QEMU case image carries the `test-exit=1`
# command line, so it is not the hash pair; after the gate, build the normal
# release image twice and compare those two. SOURCE_DATE_EPOCH and the release
# user programs keep the pair deterministic.
if ! run make -C "$clone" release-gate; then
    exit 5
fi

transcript=$clone/build/qtest/release-login.log
if [ -f "$transcript" ]; then
    cp "$transcript" "$dest/release-login.log"
    say "release-verify: qtest transcript=$dest/release-login.log"
else
    say "release-verify: release-gate produced no qtest transcript: $transcript"
    exit 5
fi

if ! run env RAVEN_USER_RELEASE=1 SOURCE_DATE_EPOCH="$commit_time" \
    make -C "$clone" image ARCH=x86_64 PROFILE=release; then
    exit 4
fi
image1=$clone/build/x86_64/ravenbsd-x86_64.img
[ -f "$image1" ] || {
    say "release-verify: first release image is missing: $image1"
    exit 4
}
cp "$image1" "$dest/image-before.img"
hash1=$(sha256sum "$dest/image-before.img" | cut -d' ' -f1)
say "release-verify: image1 sha256=$hash1"

if ! run env RAVEN_USER_RELEASE=1 SOURCE_DATE_EPOCH="$commit_time" \
    make -C "$clone" image ARCH=x86_64 PROFILE=release; then
    exit 6
fi
image2=$clone/build/x86_64/ravenbsd-x86_64.img
[ -f "$image2" ] || {
    say "release-verify: second release image is missing: $image2"
    exit 6
}
cp "$image2" "$dest/image-after.img"
hash2=$(sha256sum "$dest/image-after.img" | cut -d' ' -f1)
say "release-verify: image2 sha256=$hash2"

if [ "$hash1" != "$hash2" ]; then
    say "release-verify: FAIL image hashes differ"
    say "release-verify: before=$hash1 after=$hash2"
    exit 1
fi

say "release-verify: PASS commit=$commit sha256=$hash1"
say "release-verify: images=$dest/image-before.img $dest/image-after.img"
say "release-verify: log=$log"
exit 0
