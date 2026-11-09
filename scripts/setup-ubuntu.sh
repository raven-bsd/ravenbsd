#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host packages for Debian and Ubuntu. Called by setup.sh (honours DRY=1). Needs root or sudo.
set -eu
. "$(dirname "$0")/lib.sh"
DRY=${DRY:-0}

# Why each group:
#   build tools      host compiler/linker for build scripts, add-on recipes and the Rust toolchain build
#   qemu + firmware  run images: x86-64 and aarch64 system emulators, OVMF (x86-64 UEFI), AAVMF (aarch64 UEFI)
#   image tools      gdisk, mtools, dosfstools: only to VERIFY images (our own builder is pure Rust)
#   test tools       strace for the Linux syscall corpus, curl/git/xz for fetching
PKGS="build-essential pkg-config clang cmake ninja-build python3 git curl ca-certificates xz-utils \
libssl-dev \
qemu-system-x86 qemu-system-arm ovmf qemu-efi-aarch64 \
gdisk mtools dosfstools \
strace"

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
	if have sudo; then SUDO=sudo
	elif [ "$DRY" -eq 1 ]; then SUDO=sudo
	else die "root privileges needed: run as root or install sudo"; fi
fi

run() { if [ "$DRY" -eq 1 ]; then echo "+ $*"; else "$@"; fi; }

log "installing packages with apt-get"
run $SUDO apt-get update
# shellcheck disable=SC2086
run $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $PKGS
