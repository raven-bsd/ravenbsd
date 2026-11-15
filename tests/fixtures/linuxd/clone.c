// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B21 fixture: exercise the process/thread syscall batch of the Linux personality with raw
// syscalls, then replace the image with execve("/exec"). No stdio: musl's stdio would pull
// syscalls outside this ticket's surface. Build: musl-gcc -static -O0.

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#define PR_SET_NAME 15
#define PR_GET_NAME 16
#define ARCH_GET_FS 0x1003
#define CLONE_THREAD 0x00010000UL
#define CLONE_VM 0x00000100UL
#define CLONE_SIGHAND 0x00000800UL
#define CLONE_FS 0x00000200UL
#define CLONE_FILES 0x00000400UL
#define CLONE_SYSVSEM 0x00040000UL
#define CLONE_SETTLS 0x00080000UL
#define CLONE_PARENT_SETTID 0x00100000UL
#define CLONE_CHILD_CLEARTID 0x00200000UL
#define WNOHANG 1

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

static void fail(const char *what)
{
	out("linux-clone: FAIL ");
	out(what);
	out("\n");
	syscall(SYS_exit_group, 1);
	for (;;) {
	}
}

static void report(const char *label, long value)
{
	out("linux-clone: ");
	out(label);
	out("=");
	out_num(value);
	out("\n");
}

static int failed(long result, int wanted)
{
	return !(result == -1 && errno == wanted);
}

int main(int argc, char **argv, char **envp)
{
	(void)argc;
	(void)argv;
	(void)envp;
	out("linux-clone: start\n");

	long tid = syscall(SYS_gettid);
	long saved;
	out("linux-clone: gettid tid=");
	out_num(tid);
	out("\n");

	char clear_word;
	long set_tid = syscall(SYS_set_tid_address, &clear_word);
	out("linux-clone: set_tid_address=");
	out_num(set_tid);
	out(" same=");
	out_num(tid == set_tid);
	out("\n");
	if (tid <= 0 || tid != set_tid) {
		fail("gettid/set_tid_address");
	}

	char robust[24];
	memset(robust, 0, sizeof(robust));
	if (syscall(SYS_set_robust_list, robust, sizeof(robust)) != 0) {
		fail("set_robust_list");
	}
	out("linux-clone: set_robust_list ok=1\n");

	if (syscall(SYS_prctl, PR_SET_NAME, "clone-fixture", 0, 0, 0) != 0) {
		fail("prctl set name");
	}
	char name[16];
	memset(name, 0, sizeof(name));
	if (syscall(SYS_prctl, PR_GET_NAME, name, 0, 0, 0) != 0) {
		fail("prctl get name");
	}
	if (strcmp(name, "clone-fixture") != 0) {
		fail("prctl name mismatch");
	}
	out("linux-clone: prctl name=clone-fixture ok=1\n");

	unsigned long fs = 0;
	if (syscall(SYS_arch_prctl, ARCH_GET_FS, &fs) != 0 || fs == 0) {
		fail("arch_prctl get fs");
	}
	out("linux-clone: arch_prctl fs=");
	out_num((long)fs);
	out(" ok=1\n");

	errno = 0;
	saved = syscall(SYS_clone, 0xdeadbeefUL, 0, 0, 0, 0);
	if (failed(saved, EINVAL)) {
		fail("clone bad flags");
	}
	report("clone bad-flags errno", errno);

	errno = 0;
	saved = syscall(SYS_clone, 17 /* SIGCHLD */, 0, 0, 0, 0);
	if (failed(saved, ENOSYS)) {
		fail("clone fork-like");
	}
	report("clone fork-like errno", errno);

	volatile char stack_marker = 1;
	unsigned long child_stack = ((unsigned long)&stack_marker) & ~0xFUL;
	errno = 0;
	saved = syscall(
		SYS_clone,
		CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD | CLONE_SYSVSEM |
			CLONE_SETTLS | CLONE_PARENT_SETTID | CLONE_CHILD_CLEARTID,
		child_stack,
		0,
		0,
		0);
	if (failed(saved, ENOSYS)) {
		fail("clone thread");
	}
	report("clone thread errno", errno);

	errno = 0;
	saved = syscall(SYS_clone, CLONE_VM | CLONE_THREAD, child_stack | 8UL, 0, 0, 0);
	if (failed(saved, EINVAL)) {
		fail("clone misaligned stack");
	}
	report("clone misaligned errno", errno);

	errno = 0;
	saved = syscall(SYS_wait4, -1, NULL, 0, NULL);
	if (failed(saved, ECHILD)) {
		fail("wait4 blocking");
	}
	report("wait4 child errno", errno);

	errno = 0;
	saved = syscall(SYS_wait4, -1, NULL, WNOHANG, NULL);
	if (failed(saved, ECHILD)) {
		fail("wait4 wnochang");
	}
	report("wait4 wnochang errno", errno);

	out("linux-clone: exec start\n");
	char *exec_argv[] = { "/exec", "arg-one", NULL };
	char *exec_envp[] = { "CLONE_TEST=1", NULL };
	syscall(SYS_execve, "/exec", exec_argv, exec_envp);
	fail("execve returned");
	for (;;) {
	}
}
