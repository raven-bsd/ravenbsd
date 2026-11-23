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
# It also runs the pty-path acceptance gate (tests/ssh/pty-path-gate.py) on a test-only image built
# from the release initfs with /bin/pty-path and k449-metrics=2, recording pty-path.log,
# pty-path.exit and pty-path.img. RAVEN_K434_PTY_PATH_GATE=0 skips it.
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
mkdir -p "$out"
# K570: the release profile stages /bin/pty-path from the test fixture, and the C fixture links
# the static sysroot libc. A pre-existing fixture can be older than a just-refreshed libc (the
# K570 ttyname/isatty probe would then still be the old code). Remove it so the image/initfs
# build rebuilds it through ensure_prerequisites(), after libc-pack has refreshed the sysroot.
if [ "${RAVEN_K434_PTY_PATH_GATE:-1}" = 1 ]; then
    rm -f build/fixtures/pty-path/pty-path
fi

# The staged OpenSSH binaries are static against the Raven libc, and the image build refuses a
# stale sshd whenever libc.a is newer (any libc change, including a merged platform ticket).
# Rebuild the SSH addon here so the gate measures the current tree (measured 101.17 s at rev178).
if timeout 180 sh addons/ssh/build.sh > "$out/addons-ssh-build.log" 2>&1; then
    echo 0 > "$out/addons-ssh-build.exit"
else
    echo $? > "$out/addons-ssh-build.exit"
fi

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
cat > "$out/manifest.txt" <<EOF
tree=$tree
image=$image
image_sha256=$image_sha
runner=tests/ssh/run-k434-matrix.sh
rows=S1,S2,S3,S4,S5,S6,S7,S8,S5-session
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

if [ "${RAVEN_K434_PTY_PATH_GATE:-1}" = 1 ]; then
    echo "k434: pty-path acceptance gate"
    # The S8 qtests above rebuild build/<arch>/initfs.img for their own images, so the release
    # initfs must be rebuilt here from the release profile before the pty-path overlay. Otherwise
    # the gate boots a libc-test image with no sshd and every session fails with exit 255.
    if timeout 300 cargo xtask initfs --arch x86_64 --profile release \
        > "$out/pty-path-release-initfs.log" 2>&1; then
        echo 0 > "$out/pty-path-release-initfs.exit"
    else
        echo $? > "$out/pty-path-release-initfs.exit"
    fi
    timeout 120 cargo build -q -p raven-b70-overlay > "$out/pty-path-overlay-build.log" 2>&1 || true
    overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
    initfs=build/x86_64/initfs.img
    if [ -x "$overlay" ] && [ -f "$initfs" ] && [ "$(cat "$out/pty-path-release-initfs.exit")" = 0 ]; then
        timeout 60 "$overlay" "$initfs" "$out/pty-path-initfs.img" \
            "/bin/pty-path=$root/build/fixtures/pty-path/pty-path:0755" \
            > "$out/pty-path-overlay.log" 2>&1 || true
        timeout 600 cargo xtask image --arch x86_64 --profile release \
            --initfs "$out/pty-path-initfs.img" \
            --cmdline "console=ttyS0 test-exit=1 k449-metrics=2" \
            --output "$out/pty-path.img" > "$out/pty-path-image.log" 2>&1 || true
    fi
    if [ -f "$out/pty-path.img" ]; then
        if timeout 180 env PTY_GATE_IMAGE="$out/pty-path.img" PTY_GATE_LABEL=release \
            PTY_GATE_OUT="$out/pty-gate" PTY_GATE_EXPECT_METRICS=1 \
            PTY_GATE_SESSIONS=3 python3 tests/ssh/pty-path-gate.py \
            > "$out/pty-path.log" 2>&1; then
            echo 0 > "$out/pty-path.exit"
        else
            echo $? > "$out/pty-path.exit"
            echo "k434: pty-path gate red (see $out/pty-path.log)"
        fi
    else
        echo 1 > "$out/pty-path.exit"
        echo "k434: pty-path image missing; gate not run" > "$out/pty-path.log"
    fi
    printf 'pty_path_image=%s\npty_path_exit=%s\npty_path_initfs_exit=%s\n' \
        "$out/pty-path.img" "$(cat "$out/pty-path.exit")" \
        "$(cat "$out/pty-path-release-initfs.exit")" >> "$out/manifest.txt"
else
    echo "k434: pty-path gate skipped (RAVEN_K434_PTY_PATH_GATE=0)"
fi

RAVEN_MATRIX_ONLY=full \
RAVEN_SSH_IMAGE="$image" \
RAVEN_MATRIX_OUT="$out" \
RAVEN_MATRIX_TIMEOUT="${RAVEN_MATRIX_TIMEOUT:-780}" \
    sh tests/ssh/matrix.sh
