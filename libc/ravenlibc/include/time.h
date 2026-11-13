/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/time.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

/*
 * Portions derived from netbsd:include/time.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
 * (SPDX-License-Identifier: BSD-3-Clause), API names only.
 *
 * Copyright (c) 1989, 1993
 *	The Regents of the University of California.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
#ifndef	_TIME_H
#define _TIME_H

#include <sys/cdefs.h>

__BEGIN_DECLS

#ifndef NULL
#if __cplusplus >= 201103L
#define NULL nullptr
#elif defined(__cplusplus)
#define NULL 0L
#else
#define NULL ((void*)0)
#endif
#endif


#define __NEED_size_t
#define __NEED_time_t
#define __NEED_clock_t
#define __NEED_struct_timespec

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
#define __NEED_clockid_t
#define __NEED_timer_t
#define __NEED_pid_t
#define __NEED_locale_t
#endif

#include <bits/alltypes.h>

/* RavenBSD: tm_gmtoff and tm_zone are public on every mode (T-abi-15). */
struct tm {
	int tm_sec;
	int tm_min;
	int tm_hour;
	int tm_mday;
	int tm_mon;
	int tm_year;
	int tm_wday;
	int tm_yday;
	int tm_isdst;
	long tm_gmtoff;
	const char *tm_zone;
};

clock_t clock (void);
time_t time (time_t *);
double difftime (time_t, time_t);
time_t mktime (struct tm *);
size_t strftime (char *__restrict, size_t, const char *__restrict, const struct tm *__restrict);
struct tm *gmtime (const time_t *);
struct tm *localtime (const time_t *);
char *asctime (const struct tm *);
char *ctime (const time_t *);
int timespec_get(struct timespec *, int);

#define CLOCKS_PER_SEC 1000000L

#define TIME_UTC 1

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE

size_t strftime_l (char *  __restrict, size_t, const char *  __restrict, const struct tm *  __restrict, locale_t);

struct tm *gmtime_r (const time_t *__restrict, struct tm *__restrict);
struct tm *localtime_r (const time_t *__restrict, struct tm *__restrict);
char *asctime_r (const struct tm *__restrict, char *__restrict);
char *ctime_r (const time_t *, char *);

void tzset (void);

struct itimerspec {
	struct timespec it_interval;
	struct timespec it_value;
};

#define CLOCK_REALTIME           0
#define CLOCK_MONOTONIC          1
#define CLOCK_PROCESS_CPUTIME_ID 2
#define CLOCK_THREAD_CPUTIME_ID  3
#define CLOCK_MONOTONIC_RAW      4
#define CLOCK_REALTIME_COARSE    5
#define CLOCK_MONOTONIC_COARSE   6
#define CLOCK_BOOTTIME           7
#define CLOCK_REALTIME_ALARM     8
#define CLOCK_BOOTTIME_ALARM     9
#define CLOCK_SGI_CYCLE         10
#define CLOCK_TAI               11

/* RavenBSD: BSD clock names alias the Linux ones (doc 14 3.A.6). */
#if __BSD_VISIBLE
#define CLOCK_UPTIME            CLOCK_BOOTTIME
#define CLOCK_UPTIME_FAST       CLOCK_BOOTTIME
#define CLOCK_UPTIME_PRECISE    CLOCK_BOOTTIME
#define CLOCK_MONOTONIC_FAST    CLOCK_MONOTONIC
#define CLOCK_MONOTONIC_PRECISE CLOCK_MONOTONIC
#define CLOCK_REALTIME_FAST     CLOCK_REALTIME
#define CLOCK_REALTIME_PRECISE  CLOCK_REALTIME
#endif

#define TIMER_ABSTIME 1

int nanosleep (const struct timespec *, struct timespec *);
int clock_getres (clockid_t, struct timespec *);
int clock_gettime (clockid_t, struct timespec *);
int clock_settime (clockid_t, const struct timespec *);
int clock_nanosleep (clockid_t, int, const struct timespec *, struct timespec *);
int clock_getcpuclockid (pid_t, clockid_t *);

struct sigevent;
int timer_create (clockid_t, struct sigevent *__restrict, timer_t *__restrict);
int timer_delete (timer_t);
int timer_settime (timer_t, int, const struct itimerspec *__restrict, struct itimerspec *__restrict);
int timer_gettime (timer_t, struct itimerspec *);
int timer_getoverrun (timer_t);

extern char *tzname[2];

#endif


#if __XSI_VISIBLE || __BSD_VISIBLE
char *strptime (const char *__restrict, const char *__restrict, struct tm *__restrict);
extern int daylight;
extern long timezone;
extern int getdate_err;
struct tm *getdate (const char *);
#endif


#if __BSD_VISIBLE
int stime(const time_t *);
time_t timegm(struct tm *);
time_t timelocal(struct tm *);

/* RavenBSD: names follow netbsd:include/time.h (BSD-3-Clause, names only). */
typedef struct __state *timezone_t;
timezone_t tzalloc(const char *);
void tzfree(timezone_t);
struct tm *localtime_rz(timezone_t, const time_t *, struct tm *);
time_t mktime_z(timezone_t, struct tm *);
#endif

__END_DECLS

#endif
