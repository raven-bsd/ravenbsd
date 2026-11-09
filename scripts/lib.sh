# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Shared helpers for the scripts in this directory. Source it: `. "$(dirname "$0")/lib.sh"`.
# POSIX sh only. Replaced piece by piece by `cargo xtask` (ticket K01); the Makefile targets stay.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
export ROOT

log()  { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# Make cargo/rustup visible when they were installed by setup in this or an earlier session.
if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi

# triple_for <component> <arch>: component = kernel|user|loader
triple_for() {
	case "$1:$2" in
		kernel:x86_64|user:x86_64) echo x86_64-unknown-none ;;
		kernel:aarch64|user:aarch64) echo aarch64-unknown-none-softfloat ;;
		kernel:riscv64|user:riscv64) echo riscv64gc-unknown-none-elf ;;
		loader:x86_64) echo x86_64-unknown-uefi ;;
		loader:aarch64) echo aarch64-unknown-uefi ;;
		*) die "unsupported component/arch: $1/$2" ;;
	esac
}

# rustflags_for <component> <arch>: the flag table of tickets/CONTRACTS.md C2 (RUSTFLAGS is set here only).
rustflags_for() {
	remap="--remap-path-prefix=$ROOT=/ravenbsd"
	case "$1" in
		kernel) echo "-C relocation-model=static -C force-frame-pointers=yes $remap" ;;
		user)
			script="$ROOT/userland/linker/$2.ld"
			link=""
			[ -f "$script" ] && link="-C link-arg=-T$script"
			case "$2" in
				x86_64) echo "-C code-model=small -C relocation-model=static -C force-frame-pointers=yes $link $remap" ;;
				*) echo "-C relocation-model=static -C force-frame-pointers=yes $link $remap" ;;
			esac ;;
		loader) echo "$remap" ;;
		*) die "unknown component: $1" ;;
	esac
}

# Reproducibility environment.
repro_env() {
	if git -C "$ROOT" rev-parse HEAD >/dev/null 2>&1; then
		SOURCE_DATE_EPOCH=$(git -C "$ROOT" log -1 --format=%ct)
		short=$(git -C "$ROOT" rev-parse --short HEAD)
		dirty=""
		[ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ] && dirty="-dirty"
		date=$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%d 2>/dev/null || date -u -r "$SOURCE_DATE_EPOCH" +%Y-%m-%d)
		RAVEN_BUILD_ID="$short$dirty $date"
	else
		SOURCE_DATE_EPOCH=0
		RAVEN_BUILD_ID="dev"
	fi
	export SOURCE_DATE_EPOCH RAVEN_BUILD_ID
}

# crate_name <dir>: package name from <dir>/Cargo.toml
crate_name() {
	sed -n 's/^name *= *"\(.*\)"/\1/p' "$1/Cargo.toml" | head -n 1
}

# user_crate_dirs: directories of userland programs that exist (not libraries such as userland/rt)
user_crate_dirs() {
	for d in userland/init userland/bin/* userland/servers/* userland/drivers/*; do
		if [ -f "$ROOT/$d/Cargo.toml" ]; then echo "$d"; fi
	done
	return 0
}
