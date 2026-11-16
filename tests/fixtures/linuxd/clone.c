// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B21b fixture: exercise the Linux process/thread batch and prove that `clone` returns twice with
// `rax = 0` in the child. The child has no process identity in the trap packet, so it signals the
// parent through a shared page linuxd maps at a fixed address; the parent polls that page, prints
// one line per step and exits. No stdio: musl's stdio would pull syscalls outside this surface.
// Build: musl-gcc -static -O0.

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

#define FLAG_ADDR ((volatile unsigned long *)(uintptr_t)0x480000000000UL)
#define THREAD_MAGIC 0x5448524400000001UL
#define FORK_MAGIC 0x464f524b00000001UL

#define PR_SET_NAME 15
#define PR_GET_NAME 16
#define ARCH_GET_FS 0x1003

#define CLONE_VM 0x00000100UL
#define CLONE_FS 0x00000200UL
#define CLONE_FILES 0x00000400UL
#define CLONE_SIGHAND 0x00000800UL
#define CLONE_THREAD 0x00010000UL
#define CLONE_SYSVSEM 0x00040000UL
#define CLONE_SETTLS 0x00080000UL
#define CLONE_PARENT_SETTID 0x00100000UL
#define CLONE_CHILD_CLEARTID 0x00200000UL
#define CLONE_CHILD_SETTID 0x01000000UL
#define SIGCHLD 17

#define SPIN_LIMIT 200000000UL

static unsigned char thread_stack[64 * 1024] __attribute__((aligned(16)));

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

static void thread_child(void)
{
	FLAG_ADDR[0] = THREAD_MAGIC;
	for (;;) {
		__asm__ __volatile__("pause");
	}
}

static void fork_child(void)
{
	FLAG_ADDR[1] = FORK_MAGIC;
	for (;;) {
		__asm__ __volatile__("pause");
	}
}

/*
 * Raw clone with the Linux syscall ABI: the C fourth argument is in rcx and must move to r10.
 * The sixth argument is not a syscall argument: it stays in r9, survives the syscall in the
 * captured frame, and the child branch calls it after aligning the child stack.
 */
__attribute__((naked, noinline)) static long raw_clone(
	unsigned long flags,
	void *stack,
	int *ptid,
	int *ctid,
	unsigned long tls,
	void (*child)(void))
{
	__asm__ __volatile__(
		"movq %rcx, %r10\n\t"
		"movq $56, %rax\n\t"
		"syscall\n\t"
		"testq %rax, %rax\n\t"
		"jne 1f\n\t"
		"andq $-16, %rsp\n\t"
		"call *%r9\n\t"
		"ud2\n\t"
		"1:\n\t"
		"ret\n\t");
}

static void wait_flag(volatile unsigned long *word, unsigned long magic, const char *what)
{
	unsigned long spin;
	for (spin = 0; spin < SPIN_LIMIT; spin++) {
		if (*word == magic) {
			return;
		}
		__asm__ __volatile__("pause");
	}
	fail(what);
}

int main(int argc, char **argv, char **envp)
{
	(void)argc;
	(void)argv;
	(void)envp;
	out("linux-clone: start\n");

	long tid = syscall(SYS_gettid);
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

	long bad = raw_clone(0xdeadbeefUL, 0, 0, 0, 0, thread_child);
	if (bad != -EINVAL) {
		fail("clone bad flags");
	}
	out("linux-clone: clone bad-flags errno=22\n");

	int ptid = 0;
	int ctid = 0;
	unsigned long flags = CLONE_VM | CLONE_FS | CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD |
			      CLONE_SYSVSEM | CLONE_SETTLS | CLONE_PARENT_SETTID |
			      CLONE_CHILD_CLEARTID | CLONE_CHILD_SETTID;
	long thread_tid =
		raw_clone(flags, thread_stack + sizeof(thread_stack), &ptid, &ctid, fs, thread_child);
	if (thread_tid <= 0) {
		fail("clone thread");
	}
	out("linux-clone: clone thread pid=");
	out_num(thread_tid);
	out(" child=0\n");
	if (ptid != (int)thread_tid || ctid != (int)thread_tid) {
		fail("clone thread tid writes");
	}
	wait_flag(&FLAG_ADDR[0], THREAD_MAGIC, "clone thread flag");
	out("linux-clone: clone thread flag ok\n");

	int fork_ptid = 0;
	long child_pid = raw_clone(SIGCHLD | CLONE_PARENT_SETTID, 0, &fork_ptid, 0, 0, fork_child);
	if (child_pid <= 0) {
		fail("clone fork");
	}
	out("linux-clone: clone fork pid=");
	out_num(child_pid);
	out(" child=0\n");
	if (fork_ptid != (int)child_pid) {
		fail("clone fork ptid");
	}
	wait_flag(&FLAG_ADDR[1], FORK_MAGIC, "clone fork flag");
	out("linux-clone: clone fork flag ok\n");

	out("linux-clone: done\n");
	return 0;
}
