#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: scripts/setup.sh [--dry-run] [--check] [--donors]
#   (default)  install host packages for this distribution, install rustup and the pinned toolchain,
#              record firmware paths, verify everything
#   --dry-run  print the commands that would change the system, run nothing privileged
#   --check    only report what is present or missing (changes nothing)
#   --donors   also fetch the donor source trees (about 5.6 GB) with ../ravenbsd-resources `make setup`
# Supported: Debian/Ubuntu (scripts/setup-ubuntu.sh) and Fedora/RHEL family (scripts/setup-fedora.sh).
set -eu
. "$(dirname "$0")/lib.sh"

DRY=0; CHECK=0; DONORS=0
for a in "$@"; do
	case "$a" in
		--dry-run) DRY=1 ;;
		--check) CHECK=1 ;;
		--donors) DONORS=1 ;;
		-h|--help) sed -n '2,12p' "$0"; exit 0 ;;
		*) die "unknown option: $a" ;;
	esac
done
export DRY

if [ "$CHECK" -eq 1 ]; then
	exec sh "$ROOT/scripts/doctor.sh"
fi

[ -r /etc/os-release ] || die "cannot detect the distribution (/etc/os-release missing); supported: Debian/Ubuntu, Fedora"
# shellcheck disable=SC1091
. /etc/os-release
family="${ID:-} ${ID_LIKE:-}"
case "$family" in
	*ubuntu*|*debian*) distro=ubuntu ;;
	*fedora*|*rhel*|*centos*) distro=fedora ;;
	*) die "unsupported distribution '${ID:-unknown}'. Supported: Debian/Ubuntu and Fedora/RHEL family. Install the packages listed in scripts/setup-ubuntu.sh by hand, then run scripts/setup-rust.sh" ;;
esac

log "distribution family: $distro (${PRETTY_NAME:-$ID})"
sh "$ROOT/scripts/setup-$distro.sh"
sh "$ROOT/scripts/setup-rust.sh"

if [ "$DONORS" -eq 1 ]; then
	if [ "$DRY" -eq 1 ]; then
		echo "+ make -C $ROOT/../ravenbsd-resources setup"
	elif [ -d "$ROOT/../ravenbsd-resources" ]; then
		log "fetching donor sources (several GB, several minutes)"
		make -C "$ROOT/../ravenbsd-resources" setup
	else
		warn "../ravenbsd-resources not found; skipping donors"
	fi
fi

mkdir -p "$ROOT/build"
if [ "$DRY" -eq 0 ]; then
	sh "$ROOT/scripts/doctor.sh" --write-env
	log "setup finished. Next: make dev   (or: make check)"
else
	log "dry run finished; nothing was changed"
fi
