// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B25 fixture: exercise the Linux metadata syscall batch through raw syscalls and check the
// x86-64 `struct stat`/`struct statx` fields the synthetic tree reports. No stdio: the probe only
// writes its own lines, so the qtest can assert them verbatim.
//
// Build: musl-gcc -static -O0. The case runs this as `/hello` under linuxd.

#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

_Static_assert(sizeof(struct stat) == 144, "x86-64 struct stat");

#ifndef AT_EMPTY_PATH
#define AT_EMPTY_PATH 0x1000
#endif

#define STATX_BASIC_STATS 0x07ff

struct statx_timestamp_abi {
	int64_t tv_sec;
	uint32_t tv_nsec;
	int32_t __reserved;
};

struct statx_abi {
	uint32_t stx_mask;
	uint32_t stx_blksize;
	uint64_t stx_attributes;
	uint32_t stx_nlink;
	uint32_t stx_uid;
	uint32_t stx_gid;
	uint16_t stx_mode;
	uint16_t __spare0;
	uint64_t stx_ino;
	uint64_t stx_size;
	uint64_t stx_blocks;
	uint64_t stx_attributes_mask;
	struct statx_timestamp_abi stx_atime;
	struct statx_timestamp_abi stx_btime;
	struct statx_timestamp_abi stx_ctime;
	struct statx_timestamp_abi stx_mtime;
	uint32_t stx_rdev_major;
	uint32_t stx_rdev_minor;
	uint32_t stx_dev_major;
	uint32_t stx_dev_minor;
	uint64_t stx_mnt_id;
	uint32_t stx_dio_mem_align;
	uint32_t stx_dio_offset_align;
	uint64_t stx_subvol;
	uint32_t stx_atomic_write_unit_min;
	uint32_t stx_atomic_write_unit_max;
	uint32_t stx_atomic_write_segments_max;
	uint32_t stx_dio_read_offset_align;
	uint32_t stx_atomic_write_unit_max_opt;
	uint32_t __spare2;
	uint64_t __spare3[8];
};

_Static_assert(sizeof(struct statx_abi) == 256, "x86-64 struct statx");

#define REG_MODE 0100444
#define LNK_MODE 0120777
#define CHR_MODE 020666

static void out(const char *text)
{
	syscall(SYS_write, 1, text, strlen(text));
}

static void out_hex(unsigned long value)
{
	static const char digits[] = "0123456789abcdef";
	char buffer[18];
	int index = (int)sizeof(buffer);
	buffer[--index] = '\0';
	do {
		buffer[--index] = digits[value & 0xf];
		value >>= 4;
	} while (value != 0);
	buffer[--index] = 'x';
	buffer[--index] = '0';
	out(buffer + index);
}

static void out_num(long value)
{
	char digits[24];
	int index = (int)sizeof(digits);
	if (value == 0) {
		out("0");
		return;
	}
	if (value < 0) {
		out("-");
		value = -value;
	}
	while (value > 0 && index > 0) {
		digits[--index] = (char)('0' + value % 10);
		value /= 10;
	}
	syscall(SYS_write, 1, digits + index, sizeof(digits) - index);
}

static void fail(const char *what)
{
	out("linux-stat: FAIL ");
	out(what);
	out("\n");
	syscall(SYS_exit_group, 1);
	for (;;) {
	}
}

static void expect(long result, long expected, const char *what)
{
	if (result != expected) {
		fail(what);
	}
}

static void expect_errno(long result, int expected, const char *what)
{
	if (result != -1 || errno != expected) {
		fail(what);
	}
}

static void print_stat(const char *label, const struct stat *st)
{
	out("linux-stat: ");
	out(label);
	out(" mode=");
	out_hex(st->st_mode);
	out(" nlink=");
	out_num((long)st->st_nlink);
	out(" size=");
	out_num((long)st->st_size);
	out(" blocks=");
	out_num((long)st->st_blocks);
	out(" blksize=");
	out_num((long)st->st_blksize);
	out(" ino=");
	out_num(st->st_ino != 0);
	out("\n");
}

int main(void)
{
	struct stat st;
	struct statx_abi stx;
	int fd;
	int dirfd;

	out("linux-stat: start\n");

	/* Legacy stat/lstat: stat follows the synthetic symlink and finds no target; lstat reports it. */
	expect(syscall(SYS_stat, "/proc/self/status", &st), 0, "stat status");
	if (st.st_mode != REG_MODE || st.st_nlink != 1 || st.st_size <= 0
	    || st.st_blksize != 4096 || st.st_blocks <= 0 || st.st_ino == 0) {
		fail("stat fields");
	}
	print_stat("stat-status", &st);

	expect(syscall(SYS_lstat, "/proc/self/exe", &st), 0, "lstat exe");
	if (st.st_mode != LNK_MODE || st.st_nlink != 1 || st.st_size != 6) {
		fail("lstat fields");
	}
	print_stat("lstat-exe", &st);

	expect_errno(syscall(SYS_stat, "/proc/self/exe", &st), ENOENT, "stat follow");

	/* fstat: the standard descriptors are synthetic character devices and an open file has bytes. */
	expect(syscall(SYS_fstat, 1, &st), 0, "fstat stdout");
	if (st.st_mode != CHR_MODE || st.st_size != 0) {
		fail("fstat stdout fields");
	}
	print_stat("fstat-stdout", &st);

	fd = (int)syscall(SYS_openat, AT_FDCWD, "/proc/cpuinfo", O_RDONLY, 0);
	if (fd < 0) {
		fail("open cpuinfo");
	}
	expect(syscall(SYS_fstat, fd, &st), 0, "fstat file");
	if (st.st_mode != REG_MODE || st.st_size <= 0) {
		fail("fstat file fields");
	}
	print_stat("fstat-file", &st);
	syscall(SYS_close, fd);
	expect_errno(syscall(SYS_fstat, 99, &st), EBADF, "fstat closed");

	/* newfstatat: absolute, a directory-relative child and AT_SYMLINK_NOFOLLOW. */
	expect(syscall(SYS_newfstatat, AT_FDCWD, "/proc/self/status", &st, 0), 0, "newfstatat");
	if (st.st_mode != REG_MODE) {
		fail("newfstatat fields");
	}
	print_stat("newfstatat-status", &st);

	dirfd = (int)syscall(SYS_openat, AT_FDCWD, "/proc/self", O_RDONLY | O_DIRECTORY, 0);
	if (dirfd < 0) {
		fail("open /proc/self");
	}
	expect(syscall(SYS_newfstatat, dirfd, "status", &st, 0), 0, "newfstatat relative");
	if (st.st_mode != REG_MODE) {
		fail("newfstatat relative fields");
	}
	print_stat("newfstatat-relative", &st);
	expect(syscall(SYS_newfstatat, dirfd, "exe", &st, AT_SYMLINK_NOFOLLOW), 0,
	       "newfstatat nofollow");
	if (st.st_mode != LNK_MODE || st.st_size != 6) {
		fail("newfstatat nofollow fields");
	}
	print_stat("newfstatat-nofollow", &st);
	syscall(SYS_close, dirfd);

	expect(syscall(SYS_newfstatat, 1, "", &st, AT_EMPTY_PATH), 0, "newfstatat empty");
	if (st.st_mode != CHR_MODE) {
		fail("newfstatat empty fields");
	}
	print_stat("newfstatat-empty", &st);
	expect_errno(syscall(SYS_newfstatat, AT_FDCWD, "", &st, 0), ENOENT, "newfstatat empty no flag");

	/* statx: the basic mask and the same metadata. */
	memset(&stx, 0, sizeof(stx));
	expect(syscall(SYS_statx, AT_FDCWD, "/proc/self/status", 0, STATX_BASIC_STATS, &stx), 0,
	       "statx");
	if (stx.stx_mask != STATX_BASIC_STATS || stx.stx_mode != REG_MODE
	    || stx.stx_nlink != 1 || stx.stx_size == 0 || stx.stx_blksize != 4096) {
		fail("statx fields");
	}
	out("linux-stat: statx mask=");
	out_hex(stx.stx_mask);
	out(" mode=");
	out_hex(stx.stx_mode);
	out(" size=");
	out_num((long)stx.stx_size);
	out("\n");

	/* access: the read-only tree is readable, not writable, and directories are executable. */
	expect(syscall(SYS_access, "/proc/self/status", R_OK), 0, "access read");
	expect_errno(syscall(SYS_access, "/proc/self/status", W_OK), EACCES, "access write");
	expect_errno(syscall(SYS_access, "/proc/self/status", X_OK), EACCES, "access exec");
	expect(syscall(SYS_access, "/proc/self", X_OK), 0, "access dir exec");
	out("linux-stat: access read=0 write=EACCES exec=EACCES dir-exec=0\n");

	/* Mutators resolve the path and then report the read-only tree. */
	expect_errno(syscall(SYS_truncate, "/proc/cpuinfo", 0), EROFS, "truncate file");
	expect_errno(syscall(SYS_truncate, "/proc/self", 0), EISDIR, "truncate dir");
	expect_errno(syscall(SYS_fchmodat, AT_FDCWD, "/proc/self/status", 0444, 0), EROFS,
		     "fchmodat");
	expect_errno(syscall(SYS_fchownat, AT_FDCWD, "/proc/self/status", 0, 0, 0), EROFS,
		     "fchownat");
	expect_errno(syscall(SYS_utimensat, AT_FDCWD, "/proc/self/status", NULL, 0), EROFS,
		     "utimensat");
	out("linux-stat: mutators truncate=EROFS truncate-dir=EISDIR chmod=EROFS chown=EROFS utimes=EROFS\n");

	expect_errno(syscall(SYS_stat, "/proc/nope", &st), ENOENT, "stat missing");
	out("linux-stat: missing=ENOENT\n");
	out("linux-stat: done\n");
	syscall(SYS_exit_group, 0);
	for (;;) {
	}
}
