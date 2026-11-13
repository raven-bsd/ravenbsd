/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/resource.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_SYS_RESOURCE_H
#define	_SYS_RESOURCE_H
#include <sys/cdefs.h>
#include <bits/abi_resource.h>

__BEGIN_DECLS

#include <sys/time.h>

#define __NEED_id_t
/* RavenBSD: rlim_t is not in the committed C37 alltypes.h; declared here. */
typedef unsigned long rlim_t;

#if __BSD_VISIBLE
#define __NEED_pid_t
#endif

#include <bits/alltypes.h>
#include <bits/resource.h>

struct rlimit {
	rlim_t rlim_cur;
	rlim_t rlim_max;
};

struct rusage {
	struct timeval ru_utime;
	struct timeval ru_stime;
	/* linux extentions, but useful */
	long	ru_maxrss;
	long	ru_ixrss;
	long	ru_idrss;
	long	ru_isrss;
	long	ru_minflt;
	long	ru_majflt;
	long	ru_nswap;
	long	ru_inblock;
	long	ru_oublock;
	long	ru_msgsnd;
	long	ru_msgrcv;
	long	ru_nsignals;
	long	ru_nvcsw;
	long	ru_nivcsw;
	/* room for more... */
	long    __reserved[16];
};

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int getrlimit (int, struct rlimit *);
int setrlimit (int, const struct rlimit *);
int getrusage (int, struct rusage *);

int getpriority (int, id_t);
int setpriority (int, id_t, int);

#if __BSD_VISIBLE
int prlimit(pid_t, int, const struct rlimit *, struct rlimit *);
#define prlimit64 prlimit
#endif

#ifndef RLIMIT_RSS
#endif

#endif
__END_DECLS

#endif
