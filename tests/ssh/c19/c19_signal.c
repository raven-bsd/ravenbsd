// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// C19 conformance, first half: install a SA_SIGINFO handler, interrupt a
// nanosleep syscall with SIGALRM, and assert inside the handler that the
// ucontext holds the interrupted RIP/RSP and the correct signal number, that
// the frame is at interrupted_sp - 128, and that execution resumes at the
// interrupted instruction. Test-only; no product source is changed.

#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define UC_MCONTEXT_OFF 40UL
#define UC_SIZE 936UL
#define SIGINFO_SIZE 128UL
#define REG_RSP 15
#define REG_RIP 16

static unsigned long resume_pc;
static unsigned long sp_before;
static volatile int handler_ran;
static volatile int sig_ok;
static volatile int rip_ok;
static volatile int rsp_ok;
static volatile int frame_ok;
static unsigned long got_rip;
static unsigned long got_rsp;
static unsigned long got_uc;
static unsigned long got_si;
static unsigned long got_delta;

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
handler(int sig, siginfo_t *si, void *uc)
{
	unsigned long *gregs = (unsigned long *)((char *)uc + UC_MCONTEXT_OFF);

	got_uc = (unsigned long)uc;
	got_si = (unsigned long)si;
	got_rip = gregs[REG_RIP];
	got_rsp = gregs[REG_RSP];
	got_delta = got_uc + UC_SIZE - (got_rsp - 128);
	handler_ran = 1;
	sig_ok = si != NULL && si->si_signo == SIGALRM;
	rip_ok = got_rip == resume_pc;
	rsp_ok = got_rsp == sp_before;
	frame_ok = (got_uc + UC_SIZE == got_rsp - 128) &&
	           (got_si + SIGINFO_SIZE == got_uc);
	sayf("C19 SIGNAL handler sig=%d si_signo=%d rip=0x%lx expect_rip=0x%lx "
	     "rsp=0x%lx expect_rsp=0x%lx uc=0x%lx si=0x%lx delta=%ld "
	     "sig_ok=%d rip_ok=%d rsp_ok=%d frame_ok=%d\n",
	     sig, si != NULL ? si->si_signo : -1, got_rip, resume_pc,
	     got_rsp, sp_before, got_uc, got_si, (long)got_delta,
	     sig_ok, rip_ok, rsp_ok, frame_ok);
}

int
main(void)
{
	struct sigaction action;
	struct timespec ten = { 10, 0 };
	int resume_ran;
	int pass;
	int rc;

	memset(&action, 0, sizeof(action));
	action.sa_sigaction = handler;
	action.sa_flags = SA_SIGINFO;
	sigemptyset(&action.sa_mask);
	rc = sigaction(SIGALRM, &action, NULL);
	sayf("C19 SIGNAL sigaction rc=%d\n", rc);
	(void)alarm(1);
	__asm__ volatile(
	    "leaq 2f(%%rip), %%r11\n\t"
	    "movq %%r11, %1\n\t"
	    "movq %%rsp, %0\n\t"
	    "movl $5, %%eax\n\t"
	    "leaq %2, %%rdi\n\t"
	    "xorl %%esi, %%esi\n\t"
	    "syscall\n\t"
	    "2:\n\t"
	    "nop\n\t"
	    : "=m"(sp_before), "=m"(resume_pc)
	    : "m"(ten)
	    : "rax", "rdi", "rsi", "rcx", "r11", "memory");
	resume_ran = 1;
	pass = handler_ran && sig_ok && rip_ok && rsp_ok && frame_ok && resume_ran;
	sayf("C19 SIGNAL resume_ran=%d pass=%d\n", resume_ran, pass);
	sayf("C19 SIGNAL %s\n", pass ? "PASS" : "FAIL");
	return pass ? 0 : 1;
}
