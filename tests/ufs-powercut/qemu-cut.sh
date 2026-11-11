# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# QEMU power-cut half of B64.
#
# usage: qemu-cut.sh <ufs-image> <cut-n>
#
# This script is intentionally a documented placeholder until K55 (the UFS
# disk server) exists.  Without the guest disk server there is no way for the
# host to observe or stop the guest's N-th write, so running the QEMU half now
# would only produce a meaningless table.
#
# Planned flow once K55 is merged:
#   1. Create a qcow2 overlay backed by the committed/raw UFS image.
#   2. Boot the RavenBSD image with -snapshot and a serial log.
#   3. Run tests/ufs-powercut/workload.sh in the guest; the disk server reports
#      each completed write on the serial port.
#   4. Kill QEMU after write <cut-n> and convert the overlay back to raw.
#   5. Run fsck_ffs -n and the K57 check, then append a row to
#      build/ufs-powercut/report.tsv.
set -eu

usage() {
    echo "usage: $0 <ufs-image> <cut-n>" >&2
    exit 2
}

[ "$#" -eq 2 ] || usage
IMAGE=$1
CUT=$2

case "$CUT" in
    ''|*[!0-9]*) usage ;;
esac
[ -f "$IMAGE" ] || { echo "qemu-cut: image not found: $IMAGE" >&2; exit 2; }
command -v qemu-system-x86_64 >/dev/null 2>&1 || {
    echo "qemu-cut: qemu-system-x86_64 not installed" >&2
    exit 2
}

echo "qemu-cut: needs K55 (guest UFS disk server and write counter); cut=$CUT image=$IMAGE" >&2
echo "qemu-cut: the host-side half is in tests/ufs-powercut and is run by cargo test" >&2
exit 77
