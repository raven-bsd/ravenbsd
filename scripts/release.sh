#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Assemble and verify the RavenBSD 0.1.0 release set.
#
# Usage:
#   scripts/release.sh --check       verify the release inputs (default)
#   scripts/release.sh --candidate   build a development candidate image (base profile)
#   scripts/release.sh --build       build the release image and manifest (requires --check)
#   scripts/release.sh --verify      verify the files recorded in SHA256SUMS
#
# The script never creates a git tag. REL5c is blocked until REL2b (QEMU login), the Pi 4 board
# gate (REL3/P8/P10/P12/REL4d), REL8 clean-clone verification and the full `make check` pass; the
# owner creates the tag after reviewing this output.

set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib.sh"
cd "$ROOT"

VERSION=${VERSION:-0.1.0}
ARCH=${ARCH:-x86_64}
PROFILE=${PROFILE:-release}
CANDIDATE_PROFILE=${CANDIDATE_PROFILE:-base}
OUT=${OUT:-$ROOT/build/release/$VERSION}
MODE=check

usage() {
	cat <<'EOF'
usage: scripts/release.sh [--check|--candidate|--build|--verify]

  --check      verify the release profile, notes, QEMU/Pi 4 gates and provenance inputs
  --candidate  build a base-profile development candidate with a manifest and hash
  --build      build the release image (requires --check) and write SHA256SUMS + manifest
  --verify     verify the hashes in the output directory

Environment: VERSION (0.1.0), ARCH (x86_64), PROFILE (release), OUT.
EOF
}

while [ "$#" -gt 0 ]; do
	case "$1" in
		--check) MODE=check ;;
		--candidate) MODE=candidate ;;
		--build) MODE=build ;;
		--verify) MODE=verify ;;
		--help|-h) usage; exit 0 ;;
		*) usage >&2; die "unknown option: $1" ;;
	esac
	shift
done

mkdir -p "$OUT"

sha256_file() {
	if have sha256sum; then
		sha256sum "$1" | awk '{print $1}'
	elif have shasum; then
		shasum -a 256 "$1" | awk '{print $1}'
	else
		die "sha256sum or shasum is required"
	fi
}

need_file() {
	if [ ! -f "$ROOT/$1" ]; then
		missing="$missing $1"
	fi
}

check_inputs() {
	missing=""
	need_file "PROVENANCE.toml"
	need_file "docs/releases/$VERSION.md"
	need_file "profiles/$PROFILE.toml"

	# REL2b: automated QEMU login transcript.
	need_file "scripts/qemu-release.sh"
	need_file "docs/release-qemu.md"
	# REL3: Pi 4 release image and serial gate.
	need_file "scripts/pi4-release.sh"
	need_file "docs/pi4-release.md"
	# REL4: Pi 4 GENET link/DHCP/ping evidence.
	need_file "scripts/pi4-net-evidence.sh"
	need_file "docs/pi4-net.md"
	# REL8: independent clean-clone verification.
	need_file "scripts/release-verify.sh"
	need_file "docs/release-verify.md"

	if git rev-parse HEAD >/dev/null 2>&1 && [ -n "$(git status --porcelain)" ]; then
		warn "working tree is dirty; a release build requires a clean tree"
		missing="$missing clean-git-tree"
	fi

	if [ -n "$missing" ]; then
		printf 'release: blocked: missing%s\n' "$missing" >&2
		return 1
	fi
	log "release inputs present for $VERSION ($ARCH/$PROFILE)"
}

build_image() {
	profile=$1
	image=$2
	kind=$3
	repro_env
	SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}
	export SOURCE_DATE_EPOCH
	# The release image must carry release user programs. A debug build of the sha2 crate
	# currently trips an LLVM split-result error on x86_64-unknown-none; the release build is the
	# supported release-image path.
	export RAVEN_USER_RELEASE=1
	short=$(git rev-parse --short HEAD)
	dirty=""
	[ -n "$(git status --porcelain)" ] && dirty="-dirty"
	build_date=$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%d 2>/dev/null ||
		date -u -r "$SOURCE_DATE_EPOCH" +%Y-%m-%d)
	mkdir -p "$(dirname "$image")"
	log "building $kind image: $ARCH/$profile"
	cargo xtask image --arch "$ARCH" --profile "$profile" --output "$image"
	[ -f "$image" ] || die "image was not produced: $image"
	image_hash=$(sha256_file "$image")
	provenance_hash=$(sha256_file PROVENANCE.toml)
	rustc_version=$(rustc -Vv | sed -n 's/^release: //p')
	notes="$ROOT/docs/releases/$VERSION.md"
	cp "$notes" "$(dirname "$image")/$(basename "$VERSION").md" 2>/dev/null || true
	{
		printf 'version=%s\n' "$VERSION"
		printf 'kind=%s\n' "$kind"
		printf 'commit=%s%s\n' "$short" "$dirty"
		printf 'build_date=%s\n' "$build_date"
		printf 'arch=%s\n' "$ARCH"
		printf 'profile=%s\n' "$profile"
		printf 'image=%s\n' "$image"
		printf 'image_sha256=%s\n' "$image_hash"
		printf 'provenance_sha256=%s\n' "$provenance_hash"
		printf 'rustc=%s\n' "$rustc_version"
		printf 'notes=%s\n' "$notes"
	} >"$(dirname "$image")/manifest.txt"
	(
		cd "$(dirname "$image")"
		{
			printf '%s  %s\n' "$image_hash" "$(basename "$image")"
			if [ -f "$(basename "$VERSION").md" ]; then
				printf '%s  %s\n' "$(sha256_file "$(basename "$VERSION").md")" "$(basename "$VERSION").md"
			fi
			printf '%s  %s\n' "$(sha256_file manifest.txt)" manifest.txt
		} >SHA256SUMS
	)
	printf '%s (%s %s) %s\n' "$kind" "$ARCH" "$profile" "$image_hash"
	log "manifest: $(dirname "$image")/manifest.txt"
}

case "$MODE" in
	check)
		check_inputs
		;;
	candidate)
		log "candidate profile: $CANDIDATE_PROFILE (not the release profile)"
		build_image "$CANDIDATE_PROFILE" "$OUT/candidate-$ARCH/ravenbsd-$ARCH.img" candidate
		warn "candidate is not the 0.1.0 release artifact; the Pi 4 board gate and full make check are still required"
		;;
	build)
		check_inputs
		build_image "$PROFILE" "$OUT/ravenbsd-$ARCH.img" release
		log "release not tagged: Pi 4 board gate (REL3/P8/P10/P12/REL4d) and full make check are the gate"
		;;
	verify)
		[ -f "$OUT/SHA256SUMS" ] || die "no SHA256SUMS in $OUT"
		(cd "$OUT" && sha256sum -c SHA256SUMS)
		;;
esac
