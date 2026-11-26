#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85m: the 0.1.0 UFS image boot gate. Builds the 0.1.0-ufs image, boots it
# under QEMU for the requested accelerator, and asserts a read-write UFS root,
# a login session with a marker command and a clean poweroff. Serials, image
# hashes and the manifest are written under tickets/evidence/artifacts/S85m/.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
here_logical=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd -P)
root_logical=$(CDPATH= cd -- "$here_logical/../../.." && pwd)

find_prjmgmt() {
    base=$1
    while [ "$base" != "/" ]; do
        if [ -f "$base/ravenbsd-prjmgmt/Makefile" ]; then
            printf '%s\n' "$base/ravenbsd-prjmgmt"
            return 0
        fi
        base=$(dirname "$base")
    done
    return 1
}

arch=x86_64
accel=
mutation=none
out=

usage() {
    cat <<'EOF'
usage:
  run.sh --accel kvm|tcg [--arch x86_64] [--mutation none|root-readonly] [--out DIR]
  run.sh --help

  --accel    QEMU accelerator; kvm requires an accessible /dev/kvm
  --arch     image architecture (only x86_64 is produced by 0.1.0-ufs)
  --mutation root-readonly attaches the UFS root read-only; the gate must fail
             at the ufs-mount stage
  --out      evidence directory (default tickets/evidence/artifacts/S85m)
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --arch) arch=$2; shift 2 ;;
        --accel) accel=$2; shift 2 ;;
        --mutation) mutation=$2; shift 2 ;;
        --out) out=$2; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done

[ -n "$accel" ] || { usage >&2; exit 2; }
case "$accel" in
    kvm|tcg) ;;
    *) echo "s85m: unsupported accelerator $accel" >&2; exit 2 ;;
esac
case "$arch" in
    x86_64) ;;
    *) echo "s85m: unsupported arch $arch" >&2; exit 2 ;;
esac

if [ -z "$out" ]; then
    if [ -n "${RAVEN_PRJMGMT:-}" ]; then
        prjmgmt=$RAVEN_PRJMGMT
    else
        prjmgmt=$(find_prjmgmt "$root_logical" || find_prjmgmt "$root") || prjmgmt=
        [ -n "$prjmgmt" ] || {
            echo "s85m: cannot locate ravenbsd-prjmgmt; set RAVEN_PRJMGMT" >&2
            exit 2
        }
    fi
    out=$prjmgmt/tickets/evidence/artifacts/S85m
fi
mkdir -p "$out"

PYTHONDONTWRITEBYTECODE=1 python3 "$here/gate_ut.py" >"$out/unit-tests.log" 2>&1 || {
    echo "s85m: FAIL stage=unit-tests log=$out/unit-tests.log" >&2
    tail -n 20 "$out/unit-tests.log" >&2
    exit 1
}

esp=$root/build/s85m/$arch/0.1.0-ufs-esp.img
disk=$root/build/$arch/0.1.0-ufs-root.img
mkdir -p "$root/build/s85m/$arch"

echo "s85m: building profile 0.1.0-ufs for $arch"
if ! (cd "$root" && cargo xtask image --arch "$arch" --profile 0.1.0-ufs --output "$esp") \
    >"$out/image-build-$arch-$accel.log" 2>&1; then
    echo "s85m: FAIL stage=image log=$out/image-build-$arch-$accel.log" >&2
    tail -n 20 "$out/image-build-$arch-$accel.log" >&2
    exit 1
fi
for image in "$esp" "$disk"; do
    if [ ! -f "$image" ]; then
        echo "s85m: FAIL stage=image missing $image" >&2
        exit 1
    fi
done

set +e
python3 "$here/gate.py" \
    --arch "$arch" \
    --accel "$accel" \
    --mutation "$mutation" \
    --esp "$esp" \
    --disk "$disk" \
    --root "$root" \
    --out "$out"
status=$?
set -e

if [ "$mutation" != none ] && [ "$status" -eq 1 ]; then
    echo "mutation: $mutation rejected by the gate (expected)"
fi
exit "$status"
