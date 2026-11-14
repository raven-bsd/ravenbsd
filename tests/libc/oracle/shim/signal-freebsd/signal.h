/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * Oracle shim for freebsd:lib/libc/gen/sig2str.c and siglist.c: the BSD signal numbering
 * (HUP=1, ..., USR1=30, USR2=31), NSIG 32, and the FreeBSD real-time base 65/126. Written from
 * scratch; sys_signame[] is supplied by the donor siglist.c.
 */
#ifndef RAVEN_ORACLE_SIGNAL_FREEBSD_H
#define RAVEN_ORACLE_SIGNAL_FREEBSD_H

#include <limits.h>
#include <stdlib.h>
#include <errno.h>
#include <string.h>

#define NSIG 32
#define SIG2STR_MAX 32

#define SIGHUP 1
#define SIGINT 2
#define SIGQUIT 3
#define SIGILL 4
#define SIGTRAP 5
#define SIGABRT 6
#define SIGEMT 7
#define SIGFPE 8
#define SIGKILL 9
#define SIGBUS 10
#define SIGSEGV 11
#define SIGSYS 12
#define SIGPIPE 13
#define SIGALRM 14
#define SIGTERM 15
#define SIGURG 16
#define SIGSTOP 17
#define SIGTSTP 18
#define SIGCONT 19
#define SIGCHLD 20
#define SIGTTIN 21
#define SIGTTOU 22
#define SIGIO 23
#define SIGXCPU 24
#define SIGXFSZ 25
#define SIGVTALRM 26
#define SIGPROF 27
#define SIGWINCH 28
#define SIGINFO 29
#define SIGUSR1 30
#define SIGUSR2 31

#define SIGRTMIN 65
#define SIGRTMAX 126

extern const char *const sys_signame[NSIG];
extern const char *const sys_siglist[NSIG];
extern const int sys_nsig;

int sig2str(int signum, char *str);
int str2sig(const char *str, int *pnum);

/* FreeBSD's strtonum() is not in the host libc; this is the same observable contract for the
 * donor's use: NULL errstr on success, non-NULL and 0 on any failure. */
static inline long long
raven_oracle_strtonum(const char *s, long long min, long long max, const char **errstr)
{
	char *end = NULL;
	long long value;

	errno = 0;
	value = strtoll(s, &end, 10);
	if (end == s || end == NULL || *end != '\0' || errno != 0 || value < min || value > max) {
		*errstr = "invalid";
		return 0;
	}
	*errstr = NULL;
	return value;
}
#define strtonum(s, min, max, err) raven_oracle_strtonum((s), (min), (max), (err))

/* glibc before 2.38 has no strlcpy; the donor uses it with a fixed size. */
static inline size_t
raven_oracle_strlcpy(char *dst, const char *src, size_t size)
{
	size_t length = strlen(src);
	if (size != 0) {
		size_t copy = length < size - 1 ? length : size - 1;
		memcpy(dst, src, copy);
		dst[copy] = '\0';
	}
	return length;
}
#define strlcpy(d, s, n) raven_oracle_strlcpy((d), (s), (n))

#endif
