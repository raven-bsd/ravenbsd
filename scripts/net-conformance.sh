#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K253/K275 packet-level TCP conformance: run the raw-frame scenarios against
# the host kernel (reference) or a Raven guest over tap.  A TAP device needs
# CAP_NET_ADMIN, so the script re-executes itself in a disposable user+net+mount
# namespace.  That also isolates the Linux baseline from the host network.
#
# K275 adds `--branch <ref>` / `--worktree <path>`: the selected checkout is
# built in a detached worktree (the test-only guest fixture is overlaid when the
# branch predates K253), then the same host-side scenarios run against that
# image.  `--build-only` prints `NETCONF_IMAGE=<path>` for the matrix runner.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

die() {
    echo "net-conformance: $*" >&2
    exit 2
}

if [ "${NETCONF_NAMESPACE:-0}" != "1" ]; then
    if [ "$(id -u)" -eq 0 ]; then
        exec env NETCONF_NAMESPACE=1 "$0" "$@"
    fi
    if unshare -Urnm true 2>/dev/null; then
        exec unshare -Urnm env NETCONF_NAMESPACE=1 "$0" "$@"
    fi
    echo "net-conformance: need root or unprivileged user+net+mount namespaces for tap" >&2
    exit 77
fi

build=1
build_only=0
target=0
baseline=0
branch=
worktree=
image=
passthrough=

while [ "$#" -gt 0 ]; do
    case "$1" in
        --no-build)
            build=0
            shift
            ;;
        --build-only)
            build_only=1
            shift
            ;;
        --branch)
            [ "$#" -ge 2 ] || die "--branch needs a ref"
            branch=$2
            shift 2
            ;;
        --worktree)
            [ "$#" -ge 2 ] || die "--worktree needs a path"
            worktree=$2
            shift 2
            ;;
        --image)
            [ "$#" -ge 2 ] || die "--image needs a path"
            image=$2
            passthrough="$passthrough --image $2"
            shift 2
            ;;
        --target)
            target=1
            passthrough="$passthrough --target"
            shift
            ;;
        --baseline)
            baseline=1
            passthrough="$passthrough --baseline"
            shift
            ;;
        *)
            passthrough="$passthrough $1"
            shift
            ;;
    esac
done

if [ -n "$branch" ] || [ -n "$worktree" ]; then
    [ "$baseline" -eq 0 ] || die "--branch/--worktree cannot be combined with --baseline"
    [ -z "$image" ] || die "--branch/--worktree and --image are mutually exclusive"
    if [ "$target" -eq 0 ]; then
        target=1
        passthrough="$passthrough --target raven"
    fi
fi

if [ "$target" -eq 0 ] && [ "$baseline" -eq 0 ] && [ "$build_only" -eq 0 ]; then
    die "choose --baseline linux or --target raven"
fi

build_current() {
    fixture=${NETCONF_OUT:-build/fixtures/net-conformance}
    profile=${NETCONF_PROFILE:-network-config-test}
    image=${NETCONF_IMAGE:-build/qtest/net-conformance.img}
    sh tests/fixtures/net-conformance/build.sh
    cargo xtask image --arch x86_64 --profile "$profile" \
        --initfs "$fixture/initfs.img" \
        --output "$image"
}

build_worktree() {
    selected=$1
    selected=$(CDPATH= cd -- "$selected" && pwd)
    if [ "${NETCONF_USE_WORKTREE_FIXTURE:-0}" != "1" ]; then
        rm -rf "$selected/tests/fixtures/net-conformance"
        mkdir -p "$selected/tests/fixtures"
        cp -R "$root/tests/fixtures/net-conformance" "$selected/tests/fixtures/"
    fi
    if [ "$build" -eq 1 ]; then
        target_dir=${CARGO_TARGET_DIR:-$root/target}
        (
            cd "$selected"
            CARGO_TARGET_DIR="$target_dir" sh tests/fixtures/net-conformance/build.sh
            CARGO_TARGET_DIR="$target_dir" cargo xtask image --arch x86_64 \
                --profile network-config-test \
                --initfs build/fixtures/net-conformance/initfs.img \
                --output build/qtest/net-conformance.img
        )
    fi
    image=$selected/build/qtest/net-conformance.img
    [ -f "$image" ] || die "no image at $image (branch build did not produce one)"
}

if [ -n "$branch" ]; then
    commit=$(git rev-parse --verify --quiet "$branch^{commit}") || die "unknown ref: $branch"
    if [ -z "$worktree" ]; then
        safe=$(printf '%s' "$branch" | sed 's#[^[:alnum:]._-]#-#g')
        base=${NETCONF_WORKTREE_ROOT:-${TMPDIR:-/tmp}/opencode/k275-worktrees}
        worktree=$base/$safe
        mkdir -p "$base"
        current=
        if [ -d "$worktree" ]; then
            current=$(git -C "$worktree" rev-parse HEAD 2>/dev/null || true)
        fi
        if [ "$current" != "$commit" ]; then
            if [ -d "$worktree" ]; then
                git worktree remove --force "$worktree" >/dev/null 2>&1 || rm -rf "$worktree"
            fi
            git worktree add --detach "$worktree" "$commit" >/dev/null
        fi
    fi
    build_worktree "$worktree"
    build=0
    passthrough="$passthrough --image $image"
elif [ -n "$worktree" ]; then
    build_worktree "$worktree"
    build=0
    passthrough="$passthrough --image $image"
elif [ -n "$image" ]; then
    build=0
fi

if [ "$build_only" -eq 1 ]; then
    if [ -z "$image" ]; then
        if [ "$build" -eq 1 ]; then
            build_current
        fi
        image=${NETCONF_IMAGE:-$root/build/qtest/net-conformance.img}
    fi
    [ -f "$image" ] || die "no image at $image"
    echo "NETCONF_IMAGE=$image"
    exit 0
fi

command -v python3 >/dev/null 2>&1 || die "python3 is required"

if [ "$target" -eq 1 ]; then
    for command in ip iptables qemu-system-x86_64; do
        command -v "$command" >/dev/null 2>&1 || die "missing prerequisite: $command"
    done
    if [ ! -f /usr/share/OVMF/OVMF_CODE_4M.fd ] || [ ! -f /usr/share/OVMF/OVMF_VARS_4M.fd ]; then
        die "OVMF 4M firmware is required"
    fi
    if [ "$build" -eq 1 ]; then
        build_current
    fi
    tap=${NETCONF_TAP:-nctap0}
    ip link del "$tap" 2>/dev/null || true
    ip tuntap add dev "$tap" mode tap
    ip addr add 10.0.2.2/24 dev "$tap"
    ip link set "$tap" up
    # The host kernel has no socket for the raw connection; its unexpected RST
    # would abort the guest connection before the wire assertion.
    iptables -I OUTPUT -p tcp --dst 10.0.2.15 --tcp-flags RST RST -j DROP
fi

# shellcheck disable=SC2086
python3 tests/net-conformance/harness.py $passthrough
