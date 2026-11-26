#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Donor-independent layout check for the public filesystem records (S733).
#
# The expected numbers are written from the FreeBSD 133344405 headers selected by DR-69
# (`sys/sys/stat.h`, `sys/sys/dirent.h`, `sys/sys/statvfs.h`, `sys/sys/mount.h`) and are compiled
# into a C probe against Raven's staged public headers on both architectures. The Rust side of the
# same facts is asserted by the `raven-libc-platform` (stat/dir) and `raven-libc` (statfs) nextest
# rows. Exit codes: 0 pass, 1 mismatch, 77 clang missing.
set -eu

if ! command -v clang >/dev/null 2>&1; then
	echo "SKIP: clang not found" >&2
	exit 77
fi

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/fs-records.XXXXXX")
trap 'rm -rf "$WORK"' EXIT HUP INT TERM
RESOURCE=$(clang -print-resource-dir)

cat >"$WORK/probe.c" <<'EOF'
#include <sys/stat.h>
#include <sys/statvfs.h>
#include <sys/statfs.h>
#include <dirent.h>

_Static_assert(sizeof(struct stat) == 224, "stat size");
_Static_assert(__builtin_offsetof(struct stat, st_dev) == 0, "st_dev");
_Static_assert(__builtin_offsetof(struct stat, st_ino) == 8, "st_ino");
_Static_assert(__builtin_offsetof(struct stat, st_nlink) == 16, "st_nlink");
_Static_assert(sizeof(((struct stat *)0)->st_nlink) == 8, "st_nlink width");
_Static_assert(__builtin_offsetof(struct stat, st_mode) == 24, "st_mode");
_Static_assert(sizeof(((struct stat *)0)->st_mode) == 2, "st_mode width");
_Static_assert(__builtin_offsetof(struct stat, st_bsdflags) == 26, "st_bsdflags");
_Static_assert((((__typeof__(((struct stat *)0)->st_bsdflags))-1) < 0), "st_bsdflags signed");
_Static_assert(__builtin_offsetof(struct stat, st_uid) == 28, "st_uid");
_Static_assert(__builtin_offsetof(struct stat, st_gid) == 32, "st_gid");
_Static_assert(__builtin_offsetof(struct stat, st_padding1) == 36, "st_padding1");
_Static_assert(__builtin_offsetof(struct stat, st_rdev) == 40, "st_rdev");
_Static_assert(__builtin_offsetof(struct stat, st_atim) == 48, "st_atim");
_Static_assert(__builtin_offsetof(struct stat, st_mtim) == 64, "st_mtim");
_Static_assert(__builtin_offsetof(struct stat, st_ctim) == 80, "st_ctim");
_Static_assert(__builtin_offsetof(struct stat, st_birthtim) == 96, "st_birthtim");
_Static_assert(__builtin_offsetof(struct stat, st_size) == 112, "st_size");
_Static_assert(__builtin_offsetof(struct stat, st_blocks) == 120, "st_blocks");
_Static_assert(__builtin_offsetof(struct stat, st_blksize) == 128, "st_blksize");
_Static_assert(__builtin_offsetof(struct stat, st_flags) == 132, "st_flags");
_Static_assert(__builtin_offsetof(struct stat, st_gen) == 136, "st_gen");
_Static_assert(__builtin_offsetof(struct stat, st_filerev) == 144, "st_filerev");
_Static_assert(__builtin_offsetof(struct stat, st_spare) == 152, "st_spare");

_Static_assert(sizeof(struct dirent) == 280, "dirent size");
_Static_assert(__builtin_offsetof(struct dirent, d_fileno) == 0, "d_fileno");
_Static_assert(__builtin_offsetof(struct dirent, d_off) == 8, "d_off");
_Static_assert(__builtin_offsetof(struct dirent, d_reclen) == 16, "d_reclen");
_Static_assert(__builtin_offsetof(struct dirent, d_type) == 18, "d_type");
_Static_assert(__builtin_offsetof(struct dirent, d_pad0) == 19, "d_pad0");
_Static_assert(__builtin_offsetof(struct dirent, d_namlen) == 20, "d_namlen");
_Static_assert(__builtin_offsetof(struct dirent, d_pad1) == 22, "d_pad1");
_Static_assert(__builtin_offsetof(struct dirent, d_name) == 24, "d_name");

_Static_assert(sizeof(struct statvfs) == 88, "statvfs size");
_Static_assert(__builtin_offsetof(struct statvfs, f_bavail) == 0, "f_bavail");
_Static_assert(__builtin_offsetof(struct statvfs, f_bfree) == 8, "f_bfree");
_Static_assert(__builtin_offsetof(struct statvfs, f_blocks) == 16, "f_blocks");
_Static_assert(__builtin_offsetof(struct statvfs, f_favail) == 24, "f_favail");
_Static_assert(__builtin_offsetof(struct statvfs, f_ffree) == 32, "f_ffree");
_Static_assert(__builtin_offsetof(struct statvfs, f_files) == 40, "f_files");
_Static_assert(__builtin_offsetof(struct statvfs, f_bsize) == 48, "f_bsize");
_Static_assert(__builtin_offsetof(struct statvfs, f_flag) == 56, "f_flag");
_Static_assert(__builtin_offsetof(struct statvfs, f_frsize) == 64, "f_frsize");
_Static_assert(__builtin_offsetof(struct statvfs, f_fsid) == 72, "f_fsid");
_Static_assert(__builtin_offsetof(struct statvfs, f_namemax) == 80, "f_namemax");

_Static_assert(sizeof(struct statfs) == 2344, "statfs size");
_Static_assert(__builtin_offsetof(struct statfs, f_version) == 0, "f_version");
_Static_assert(__builtin_offsetof(struct statfs, f_type) == 4, "f_type");
_Static_assert(__builtin_offsetof(struct statfs, f_flags) == 8, "f_flags");
_Static_assert(__builtin_offsetof(struct statfs, f_bsize) == 16, "f_bsize");
_Static_assert(__builtin_offsetof(struct statfs, f_iosize) == 24, "f_iosize");
_Static_assert(__builtin_offsetof(struct statfs, f_blocks) == 32, "f_blocks");
_Static_assert(__builtin_offsetof(struct statfs, f_bfree) == 40, "f_bfree");
_Static_assert(__builtin_offsetof(struct statfs, f_bavail) == 48, "f_bavail");
_Static_assert(__builtin_offsetof(struct statfs, f_files) == 56, "f_files");
_Static_assert(__builtin_offsetof(struct statfs, f_ffree) == 64, "f_ffree");
_Static_assert(__builtin_offsetof(struct statfs, f_spare) == 112, "f_spare");
_Static_assert(__builtin_offsetof(struct statfs, f_namemax) == 184, "f_namemax");
_Static_assert(__builtin_offsetof(struct statfs, f_owner) == 188, "f_owner");
_Static_assert(__builtin_offsetof(struct statfs, f_fsid) == 192, "f_fsid");
_Static_assert(__builtin_offsetof(struct statfs, f_charspare) == 200, "f_charspare");
_Static_assert(__builtin_offsetof(struct statfs, f_fstypename) == 280, "f_fstypename");
_Static_assert(__builtin_offsetof(struct statfs, f_mntfromname) == 296, "f_mntfromname");
_Static_assert(__builtin_offsetof(struct statfs, f_mntonname) == 1320, "f_mntonname");

int main(void) { return 0; }
EOF

for arch in x86_64 aarch64; do
	if ! clang --target="$arch-unknown-ravenbsd" -nostdinc \
		-isystem "$RESOURCE/include" \
		-isystem "$ROOT/libc/ravenlibc/include" \
		-isystem "$ROOT/libc/ravenlibc/abi/generated/include" \
		-isystem "$ROOT/libc/ravenlibc/arch/$arch/include" \
		-std=c11 -Werror -fsyntax-only "$WORK/probe.c" 2>"$WORK/$arch.log"; then
		echo "FAIL: $arch public records do not match the FreeBSD profile" >&2
		sed -n '1,20p' "$WORK/$arch.log" >&2
		exit 1
	fi
	echo "ok: $arch stat/dirent/statvfs/statfs records"
done
