#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K434 single-shot gate runner. One tree hash, one image, one command:
#   sh tests/ssh/run-k434-matrix.sh
#
# It builds build/qtest/ssh-k434-full.img from the current HEAD when the image (or its
# <image>.tree marker) is absent, records the tree/image sha256 in <out>/manifest.txt, applies the
# functional hygiene gate, then runs the merged matrix once with RAVEN_MATRIX_ONLY=full. The full
# mode runs S1, S2, S3, S4, S5/S6 (including S5-session), S7 and S8 in order on that image and writes
# serial-S1.log ... serial-S8.log deltas.
#
# The inner matrix cap is matrix.sh's RAVEN_MATRIX_TIMEOUT (default 780 s). The expected full run is
# about 10 minutes from the measured rows (S7 5:08, S8 1:20, S5-session 0:34, plus boot and S1-S4);
# 780 s is that expectation plus margin. The image build cap is 900 s because the measured build is
# 11:00.36 (rev 50).
#
# Environment:
#   RAVEN_SSH_IMAGE       image path            (default build/qtest/ssh-k434-full.img)
#   RAVEN_MATRIX_OUT      artifact directory    (default build/ssh/k434-full)
#   RAVEN_QEMU_HYGIENE    hygiene script        (default ../../../ravenbsd-prjmgmt/scripts/...)
#   RAVEN_MATRIX_TIMEOUT  inner matrix cap      (default 780)
#   RAVEN_K434_ALLOW_STALE=1  reuse an image whose .tree marker differs from HEAD

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

image=${RAVEN_SSH_IMAGE:-build/qtest/ssh-k434-full.img}
out=${RAVEN_MATRIX_OUT:-build/ssh/k434-full}
hygiene=${RAVEN_QEMU_HYGIENE:-}
if [ -z "$hygiene" ]; then
    for candidate in \
        "$root/../../../ravenbsd-prjmgmt/scripts/qemu-hygiene.sh" \
        "$HOME/work/ravenos-workspace/ravenbsd-prjmgmt/scripts/qemu-hygiene.sh"; do
        if [ -f "$candidate" ]; then
            hygiene=$candidate
            break
        fi
    done
fi
tree=$(git rev-parse HEAD)
marker=$image.tree

if [ ! -f "$image" ]; then
    echo "k434: building one image from tree $tree"
    timeout 900 cargo xtask image --arch x86_64 --profile release \
        --cmdline "console=ttyS0 test-exit=1" --output "$image"
    printf '%s\n' "$tree" > "$marker"
elif [ -f "$marker" ]; then
    built_tree=$(cat "$marker")
    if [ "$built_tree" != "$tree" ] && [ "${RAVEN_K434_ALLOW_STALE:-0}" != 1 ]; then
        echo "k434: $image was built from $built_tree, tree is $tree" >&2
        echo "k434: rebuild it or set RAVEN_K434_ALLOW_STALE=1 for a deliberate reuse" >&2
        exit 2
    fi
else
    echo "k434: no $marker; assuming $image matches tree $tree" >&2
    printf '%s\n' "$tree" > "$marker"
fi

image_sha=$(sha256sum "$image" | awk '{print $1}')
mkdir -p "$out"
cat > "$out/manifest.txt" <<EOF
tree=$tree
image=$image
image_sha256=$image_sha
runner=tests/ssh/run-k434-matrix.sh
rows=S1,S2,S3,S4,S4-echo,S4-exit0,S5,S6,S7,S8,S5-session
command=RAVEN_MATRIX_ONLY=full RAVEN_SSH_IMAGE=$image RAVEN_MATRIX_OUT=$out sh tests/ssh/matrix.sh
EOF
cat "$out/manifest.txt"

if [ -x "$hygiene" ] || [ -f "$hygiene" ]; then
    if ! timeout 20 sh "$hygiene" > "$out/hygiene.log" 2>&1; then
        cat "$out/hygiene.log"
        echo "k434: hygiene refused; not starting the matrix" >&2
        exit 1
    fi
    cat "$out/hygiene.log"
    grep -q '^OK:' "$out/hygiene.log" || {
        echo "k434: hygiene did not print OK; not starting the matrix" >&2
        exit 1
    }
else
    echo "k434: WARNING hygiene script not found at $hygiene" >&2
fi

if [ "${RAVEN_K434_PTY_QTEST:-1}" = 1 ]; then
    echo "k434: S8 auxiliary qtests (pty-alloc, termios)"
    if timeout 180 cargo xtask qtest --case pty-alloc --arch x86_64 \
        > "$out/pty-alloc.log" 2>&1; then
        echo 0 > "$out/pty-alloc.exit"
    else
        echo $? > "$out/pty-alloc.exit"
    fi
    if timeout 150 cargo xtask qtest --case termios --arch x86_64 \
        > "$out/termios.log" 2>&1; then
        echo 0 > "$out/termios.exit"
    else
        echo $? > "$out/termios.exit"
    fi
    timeout 120 sh tests/fixtures/ssh-openpty/build.sh || true
else
    echo "k434: S8 auxiliary qtests skipped (RAVEN_K434_PTY_QTEST=0)"
fi

RAVEN_MATRIX_ONLY=full \
RAVEN_SSH_IMAGE="$image" \
RAVEN_MATRIX_OUT="$out" \
RAVEN_MATRIX_TIMEOUT="${RAVEN_MATRIX_TIMEOUT:-780}" \
    sh tests/ssh/matrix.sh
