// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B23 fixture: exercise linuxd's file syscall batch through the `file:` namespace.
//
// The initfs carries `/data.txt` (read through vfsd -> initfsd) and a ramfs mounted at `/tmp`
// (read/write through vfsd -> ramfsd). The probe opens, reads, writes, seeks, preads, pwrites,
// fcntls and dup3s a file with raw syscalls and prints one line per step. Build: musl-gcc
// -static -O0.

#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#define AT_FDCWD_ (-100)

static void out(const char *text)
{
	syscall(SYS_write, 1, text, strlen(text));
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

static void fail(const char *what, long result)
{
	out("linux-files: FAIL ");
	out(what);
	out(" result=");
	out_num(result);
	out("\n");
	syscall(SYS_exit_group, 1);
	for (;;) {
	}
}

static long check(long result, const char *what)
{
	if (result < 0) {
		fail(what, result);
	}
	return result;
}

static void expect(long result, long wanted, const char *what)
{
	if (result != wanted) {
		fail(what, result);
	}
}

int main(int argc, char **argv, char **envp)
{
	(void)argc;
	(void)argv;
	(void)envp;
	char buffer[64];

	out("linux-files: start\n");

	long fd = check(
		syscall(SYS_openat, AT_FDCWD_, "/data.txt", O_RDONLY, 0),
		"open data.txt");
	long read = check(syscall(SYS_read, fd, buffer, sizeof(buffer)), "read data.txt");
	if (read <= 0) {
		fail("empty data.txt", read);
	}
	out("linux-files: read initfs ok bytes=");
	out_num(read);
	out("\n");
	check(syscall(SYS_close, fd), "close data.txt");

	fd = check(
		syscall(SYS_openat, AT_FDCWD_, "/tmp/b23.txt",
			O_CREAT | O_RDWR | O_TRUNC, 0644),
		"open tmp");
	expect(syscall(SYS_write, fd, "b23-file", 8), 8, "write tmp");
	expect(syscall(SYS_lseek, fd, 0, SEEK_SET), 0, "lseek start");
	expect(syscall(SYS_lseek, fd, 0, SEEK_END), 8, "lseek end");
	expect(syscall(SYS_lseek, fd, 0, SEEK_SET), 0, "lseek rewind");
	memset(buffer, 0, sizeof(buffer));
	expect(syscall(SYS_read, fd, buffer, 8), 8, "read tmp");
	if (memcmp(buffer, "b23-file", 8) != 0) {
		fail("read mismatch", -1);
	}
	out("linux-files: write/read ok\n");

	expect(syscall(SYS_pwrite64, fd, "XY", 2, 2), 2, "pwrite tmp");
	memset(buffer, 0, sizeof(buffer));
	expect(syscall(SYS_pread64, fd, buffer, 2, 2), 2, "pread tmp");
	if (memcmp(buffer, "XY", 2) != 0) {
		fail("pread mismatch", -1);
	}
	out("linux-files: pread/pwrite ok\n");

	long flags = check(syscall(SYS_fcntl, fd, F_GETFL, 0), "fcntl getfl");
	if ((flags & O_ACCMODE) != O_RDWR) {
		fail("fcntl flags", flags);
	}
	expect(syscall(SYS_fcntl, fd, F_SETFD, FD_CLOEXEC), 0, "fcntl setfd");
	expect(syscall(SYS_fcntl, fd, F_GETFD, 0), FD_CLOEXEC, "fcntl getfd");
	out("linux-files: fcntl ok\n");

	long duplicate = check(
		syscall(SYS_dup3, fd, 9, O_CLOEXEC), "dup3");
	expect(duplicate, 9, "dup3 fd");
	expect(syscall(SYS_fcntl, 9, F_GETFD, 0), FD_CLOEXEC, "dup3 cloexec");
	expect(syscall(SYS_lseek, fd, 0, SEEK_SET), 0, "lseek before dup read");
	memset(buffer, 0, sizeof(buffer));
	expect(syscall(SYS_read, 9, buffer, 8), 8, "read dup");
	if (memcmp(buffer, "b2XYfile", 8) != 0) {
		fail("dup read mismatch", -1);
	}
	out("linux-files: dup3 ok\n");
	check(syscall(SYS_close, 9), "close dup");
	check(syscall(SYS_close, fd), "close tmp");

	fd = check(
		syscall(SYS_openat, AT_FDCWD_, "/tmp/b23.txt", O_RDONLY, 0),
		"reopen tmp");
	memset(buffer, 0, sizeof(buffer));
	expect(syscall(SYS_read, fd, buffer, 8), 8, "reread tmp");
	if (memcmp(buffer, "b2XYfile", 8) != 0) {
		fail("reread mismatch", -1);
	}
	check(syscall(SYS_close, fd), "close reopen");
	out("linux-files: reopen ok\n");

	out("linux-files: done\n");
	return 0;
}
