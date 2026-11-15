// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// B21 execve target: reached only through the personality's execve image replacement. Checks the
// new argv/envp built from the caller's pointers and exits with a distinct status. No stdio.

#include <stddef.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

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

static void fail(const char *what)
{
	out("linux-exec: FAIL ");
	out(what);
	out("\n");
	syscall(SYS_exit_group, 1);
	for (;;) {
	}
}

int main(int argc, char **argv, char **envp)
{
	if (argc != 2) {
		fail("argc");
	}
	if (strcmp(argv[0], "/exec") != 0 || strcmp(argv[1], "arg-one") != 0) {
		fail("argv");
	}
	if (envp == NULL || envp[0] == NULL || strcmp(envp[0], "CLONE_TEST=1") != 0) {
		fail("envp");
	}
	out("linux-exec: target ok pid=");
	out_num((long)syscall(SYS_getpid));
	out(" tid=");
	out_num((long)syscall(SYS_gettid));
	out(" argv1=");
	out(argv[1]);
	out(" env=");
	out(envp[0]);
	out("\n");
	syscall(SYS_exit_group, 7);
	for (;;) {
	}
}
