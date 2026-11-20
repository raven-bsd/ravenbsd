#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host-side SSH login test over the K227 guest network (no hostfwd substitution).
# Set SSH_GUEST_HOST to the guest address published by K227; optional SSH_GUEST_PORT (default 22).

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

if [ -z "${SSH_GUEST_HOST:-}" ]; then
    echo "ssh-guest-login: SSH_GUEST_HOST is not set; waiting for K227" >&2
    exit 2
fi

exec python3 tests/ssh/ssh-guest-login.py
