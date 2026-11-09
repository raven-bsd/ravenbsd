#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host packages for Fedora (and the RHEL family with EPEL). Called by setup.sh (honours DRY=1). Needs root or sudo.
set -eu
. "$(dirname "$0")/lib.sh"
DRY=${DRY:-0}

# Same groups as setup-ubuntu.sh; Fedora names:
#   edk2-ovmf = x86-64 UEFI firmware, edk2-aarch64 = AAVMF, gdisk provides sgdisk.
PKGS="gcc gcc-c++ make pkgconf-pkg-config clang cmake ninja-build python3 git curl ca-certificates xz \
openssl-devel \
qemu-system-x86-core qemu-system-aarch64-core edk2-ovmf edk2-aarch64 \
gdisk mtools dosfstools \
strace"

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
	if have sudo; then SUDO=sudo
	elif [ "$DRY" -eq 1 ]; then SUDO=sudo
	else die "root privileges needed: run as root or install sudo"; fi
fi

run() { if [ "$DRY" -eq 1 ]; then echo "+ $*"; else "$@"; fi; }

PM=dnf
have dnf || PM=yum
log "installing packages with $PM"
# shellcheck disable=SC2086
run $SUDO $PM install -y $PKGS
