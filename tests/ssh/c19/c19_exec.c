// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// C19 conformance, second half: stage 1 installs a SIGUSR1 handler and execs
// the same program; stage 2 reads the disposition (must be SIG_DFL after exec),
// installs its own handler, raises SIGUSR1 and asserts the old handler never
// ran. Test-only; no product source is changed.

#include <signal.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static volatile int old_ran;
static volatile int new_ran;

static long
debug_write(const char *bytes, unsigned long len)
{
	register long number asm("rax") = 3;
	register const char *pointer asm("rdi") = bytes;
	register unsigned long length asm("rsi") = len;

	__asm__ volatile("syscall"
	                 : "+r"(number)
	                 : "r"(pointer), "r"(length)
	                 : "rcx", "r11", "memory");
	return number;
}

static void
sayf(const char *format, ...)
{
	char line[320];
	va_list arguments;
	int length;

	va_start(arguments, format);
	length = vsnprintf(line, sizeof(line), format, arguments);
	va_end(arguments);
	if (length > 0)
		(void)debug_write(line, (unsigned long)length);
}

static void
old_handler(int sig)
{
	old_ran = 1;
	sayf("C19 EXEC OLD HANDLER RAN sig=%d\n", sig);
}

static void
new_handler(int sig)
{
	new_ran = 1;
	sayf("C19 EXEC NEW HANDLER RAN sig=%d\n", sig);
}

static int
child_stage(void)
{
	struct sigaction current;
	struct sigaction action;
	int read_rc;
	int install_rc;
	int raise_rc;
	int pass;

	memset(&current, 0, sizeof(current));
	read_rc = sigaction(SIGUSR1, NULL, &current);
	sayf("C19 EXEC child disposition_rc=%d handler=%p dfl=%d ign=%d old=%d\n",
	     read_rc, (void *)current.sa_handler, current.sa_handler == SIG_DFL,
	     current.sa_handler == SIG_IGN, current.sa_handler == old_handler);
	memset(&action, 0, sizeof(action));
	action.sa_handler = new_handler;
	sigemptyset(&action.sa_mask);
	install_rc = sigaction(SIGUSR1, &action, NULL);
	raise_rc = raise(SIGUSR1);
	sayf("C19 EXEC child install_rc=%d raise_rc=%d new_ran=%d old_ran=%d\n",
	     install_rc, raise_rc, new_ran, old_ran);
	pass = read_rc == 0 && current.sa_handler == SIG_DFL && new_ran && !old_ran;
	sayf("C19 EXEC %s\n", pass ? "PASS" : "FAIL");
	return pass ? 0 : 1;
}

int
main(int argc, char **argv)
{
	struct sigaction action;
	char *next[] = { (char *)"/bin/c19-exec", (char *)"child", NULL };

	if (argc > 1 && strcmp(argv[1], "child") == 0)
		return child_stage();
	memset(&action, 0, sizeof(action));
	action.sa_handler = old_handler;
	sigemptyset(&action.sa_mask);
	(void)sigaction(SIGUSR1, &action, NULL);
	sayf("C19 EXEC stage1 installed old handler\n");
	execv("/bin/c19-exec", next);
	sayf("C19 EXEC execv failed\n");
	return 1;
}
