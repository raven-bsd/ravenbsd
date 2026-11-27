/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/wait.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 * The idtype values and W* selectors/status macros follow the FreeBSD profile
 * (sys/sys/wait.h at 133344405f532bd54ff3629a77d4c4e062f01fdb, BSD-3-Clause).
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
#ifndef	_SYS_WAIT_H
#define	_SYS_WAIT_H
#include <sys/cdefs.h>
__BEGIN_DECLS

#define __NEED_pid_t
#define __NEED_id_t
#include <bits/alltypes.h>

typedef enum {
	/* FreeBSD sys/sys/wait.h keeps the Solaris-derived numbering. */
	P_PID = 0,
	P_PGID = 2,
	P_ALL = 7
} idtype_t;

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
pid_t wait (int *);
pid_t waitpid (pid_t, int *, int );

#if __POSIX_VISIBLE \
 || __XSI_VISIBLE || __BSD_VISIBLE \
 || __BSD_VISIBLE
#include <signal.h>
int waitid (idtype_t, id_t, siginfo_t *, int);
#endif

#include <sys/resource.h>
#if __BSD_VISIBLE
/* FreeBSD sys/sys/wait.h:167 (BSD-3-Clause, donor 133344405). */
pid_t wait6 (idtype_t, id_t, int *, int, struct __wrusage *, siginfo_t *);
#endif
pid_t wait3 (int *, int, struct rusage *);
pid_t wait4 (pid_t, int *, int, struct rusage *);

#define WNOHANG    1
#define WUNTRACED  2

#define WSTOPPED   2
#define WEXITED    16
#define WCONTINUED 4
#define WNOWAIT    8

#define __WALL      0x40000000

#define WEXITSTATUS(s) (((s) & 0xff00) >> 8)
#define WTERMSIG(s) ((s) & 0x7f)
#define WSTOPSIG(s) WEXITSTATUS(s)
#define WCOREDUMP(s) ((s) & 0x80)
#define WIFEXITED(s) (((s) & 0x7f) == 0)
#define WIFSTOPPED(s) (((s) & 0x7f) == 0x7f)
#define WIFSIGNALED(s) (((s) & 0x7f) != 0x7f && ((s) & 0x7f) != 0 && (s) != 0x13)
#define WIFCONTINUED(s) ((s) == 0x13)

#endif
__END_DECLS
#endif
