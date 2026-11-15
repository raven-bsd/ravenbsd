// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B30 fixture: read every synthetic procfs/sysfs file with the raw Linux syscalls the
// personality server serves, and check the documented shape of each. No stdio: musl's stdio
// would pull syscalls outside this ticket's surface (fstat, fcntl, ...).
//
// Build: musl-gcc -static -O0. The qtest asserts the printed lines verbatim.

#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#define BUFSZ 4096

struct linux_dirent64 {
	uint64_t d_ino;
	int64_t d_off;
	unsigned short d_reclen;
	unsigned char d_type;
	char d_name[];
};

static void die(const char *what)
{
	static const char prefix[] = "linux-proc: FAIL ";
	syscall(SYS_write, 1, prefix, sizeof(prefix) - 1);
	syscall(SYS_write, 1, what, strlen(what));
	syscall(SYS_write, 1, "\n", 1);
	syscall(SYS_exit_group, 1);
	for (;;) {
	}
}

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
	while (value > 0 && index > 0) {
		digits[--index] = (char)('0' + value % 10);
		value /= 10;
	}
	syscall(SYS_write, 1, digits + index, sizeof(digits) - index);
}

static int has(const char *buffer, long length, const char *needle)
{
	long needle_len = (long)strlen(needle);
	if (needle_len == 0 || needle_len > length) {
		return 0;
	}
	for (long i = 0; i + needle_len <= length; i++) {
		if (memcmp(buffer + i, needle, needle_len) == 0) {
			return 1;
		}
	}
	return 0;
}

static long read_file(const char *path, char *buffer, long capacity)
{
	int fd = (int)syscall(SYS_openat, AT_FDCWD, path, O_RDONLY, 0);
	if (fd < 0) {
		return fd;
	}
	long total = 0;
	while (total < capacity) {
		long got = syscall(SYS_read, fd, buffer + total, capacity - total);
		if (got < 0) {
			syscall(SYS_close, fd);
			return got;
		}
		if (got == 0) {
			break;
		}
		total += got;
	}
	syscall(SYS_close, fd);
	return total;
}

static void check_file(const char *path, const char *needle)
{
	char buffer[BUFSZ];
	long length = read_file(path, buffer, sizeof(buffer));
	if (length < 0) {
		die(path);
	}
	if (!has(buffer, length, needle)) {
		die(path);
	}
	out("linux-proc: ");
	out(path);
	out(" ok ");
	out_num(length);
	out(" bytes\n");
}

static void check_fd_directory(void)
{
	char buffer[BUFSZ];
	int fd = (int)syscall(SYS_openat, AT_FDCWD, "/proc/self/fd", O_RDONLY | O_DIRECTORY, 0);
	if (fd < 0) {
		die("/proc/self/fd");
	}
	int saw0 = 0, saw1 = 0, saw2 = 0;
	for (;;) {
		long got = syscall(SYS_getdents64, fd, buffer, sizeof(buffer));
		if (got < 0) {
			die("/proc/self/fd getdents64");
		}
		if (got == 0) {
			break;
		}
		for (long offset = 0; offset < got;) {
			struct linux_dirent64 *entry = (struct linux_dirent64 *)(buffer + offset);
			if (entry->d_reclen == 0 || offset + entry->d_reclen > got) {
				die("/proc/self/fd record");
			}
			if (strcmp(entry->d_name, "0") == 0) {
				saw0 = 1;
			} else if (strcmp(entry->d_name, "1") == 0) {
				saw1 = 1;
			} else if (strcmp(entry->d_name, "2") == 0) {
				saw2 = 1;
			}
			offset += entry->d_reclen;
		}
	}
	syscall(SYS_close, fd);
	if (!saw0 || !saw1 || !saw2) {
		die("/proc/self/fd names");
	}
	out("linux-proc: /proc/self/fd ok\n");
}

static void check_exe(void)
{
	char buffer[128];
	long length = syscall(SYS_readlinkat, AT_FDCWD, "/proc/self/exe", buffer, sizeof(buffer));
	if (length < 0) {
		die("/proc/self/exe");
	}
	if (!has(buffer, length, "/hello")) {
		die("/proc/self/exe target");
	}
	out("linux-proc: /proc/self/exe ok ");
	out_num(length);
	out(" bytes\n");
}

int main(void)
{
	out("linux-proc: start\n");
	check_file("/proc/self/status", "Name:\t");
	check_file("/proc/self/maps", "rw-p");
	check_file("/proc/self/status", "Pid:\t");
	check_file("/proc/self/cmdline", "/hello");
	check_file("/proc/self/stat", "(hello)");
	check_file("/proc/cpuinfo", "processor");
	check_file("/proc/meminfo", "MemTotal:");
	check_file("/sys/devices/system/cpu/online", "0");
	check_fd_directory();
	check_exe();

	int fd = (int)syscall(SYS_openat, AT_FDCWD, "/proc/nope", O_RDONLY, 0);
	if (fd >= 0) {
		syscall(SYS_close, fd);
		die("/proc/nope opened");
	}
	out("linux-proc: enoent ok\n");
	out("linux-proc: done\n");
	syscall(SYS_exit_group, 0);
	for (;;) {
	}
}
