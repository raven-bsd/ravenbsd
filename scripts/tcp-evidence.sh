#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K264 TCP provenance and evidence runner. The implementation is tcp-evidence.py; this wrapper
# keeps the repository convention (`sh scripts/<name>.sh`) and runs it in the foreground.
#
#   sh scripts/tcp-evidence.sh --selftest
#   sh scripts/tcp-evidence.sh --stress --cycles 1 --mode kvm2
#   sh scripts/tcp-evidence.sh --audit --reports-root /path/to/ravenbsd-prjmgmt/tickets \
#       --freebsd-root /path/to/freebsd --dragonfly-root /path/to/dragonfly
#   sh scripts/tcp-evidence.sh --all --reports-root /path/to/ravenbsd-prjmgmt/tickets \
#       --freebsd-root /path/to/freebsd --dragonfly-root /path/to/dragonfly

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

command -v python3 >/dev/null 2>&1 || {
    echo "tcp-evidence: python3 is required" >&2
    exit 77
}

# K525: the stress subcommand is the preflighted runner; it is dispatched before the
# provenance/evidence phases so a missing image input fails in preflight, not in a cap.
if [ "${1:-}" = "--stress" ]; then
    shift
    exec python3 "$root/tests/tcpip/stress/stress_preflight.py" "$@"
fi

exec python3 "$root/scripts/tcp-evidence.py" "$@"
