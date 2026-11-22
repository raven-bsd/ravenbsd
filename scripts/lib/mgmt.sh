#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# One resolver for product programs that write management evidence:
# RAVEN_MGMT_ROOT when set, otherwise ../ravenbsd-prjmgmt relative to the
# product checkout. Missing is a hard error naming RAVEN_MGMT_ROOT; there is
# no fallback to the frozen product trees.

mgmt_root() {
    if [ -n "${RAVEN_MGMT_ROOT:-}" ]; then
        printf '%s\n' "$RAVEN_MGMT_ROOT"
    else
        root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
        printf '%s\n' "$root/ravenbsd-prjmgmt"
    fi
}

mgmt_require_root() {
    root=$(mgmt_root)
    if [ ! -d "$root" ]; then
        printf 'error: RAVEN_MGMT_ROOT: management root %s is not a directory; set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout (no product docs fallback)\n' "$root" >&2
        return 1
    fi
    printf '%s\n' "$root"
}

mgmt_path() {
    root=$(mgmt_require_root) || return 1
    printf '%s/%s\n' "$root" "$1"
}

mgmt_require_file() {
    path=$(mgmt_path "$1") || return 1
    if [ ! -f "$path" ]; then
        printf 'error: RAVEN_MGMT_ROOT: missing management file %s; set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout (no product docs fallback)\n' "$path" >&2
        return 1
    fi
    printf '%s\n' "$path"
}
