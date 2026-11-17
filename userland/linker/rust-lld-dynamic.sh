#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# rust-lld wrapper for dynamic x86_64 and aarch64 user programs.
#
# The built-in bare-metal targets pass `-static` to rust-lld, which cannot be undone with
# `-Bdynamic`. This wrapper removes only that flag (and the equivalent `--no-pie`) and forwards
# every other argument unchanged so the dynamic `-pie`/`-dynamic-linker` flags take effect. It is
# used only by the dynamic-userland build path; static builds keep the default linker.

set -euo pipefail

sysroot=$(rustc --print sysroot)
host=$(rustc -vV | sed -n 's/^host: //p')
lld="$sysroot/lib/rustlib/$host/bin/rust-lld"
if [[ ! -x "$lld" ]]; then
    lld=$(command -v rust-lld || true)
fi
if [[ -z "$lld" || ! -x "$lld" ]]; then
    echo "rust-lld-dynamic: rust-lld not found" >&2
    exit 1
fi

args=()
for arg in "$@"; do
    case "$arg" in
        -static | -static-pie | --no-pie) ;;
        *) args+=("$arg") ;;
    esac
done

exec "$lld" "${args[@]}"
