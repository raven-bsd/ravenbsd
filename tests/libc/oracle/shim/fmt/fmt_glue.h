/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Late glue for the FreeBSD printf oracle, written from scratch.  It is pulled
 * in by the replacement <math.h> that printfcommon.h includes after "local.h"
 * and "fvwrite.h", so the weak definitions below see the complete
 * struct __suio.
 */
#ifndef RL_FMT_GLUE_H
#define RL_FMT_GLUE_H

#ifdef RL_FMT_STDIO_H

#include <string.h>
#include <locale.h>
#include <wchar.h>

__attribute__((weak)) int
__swsetup(FILE *fp)
{
	if ((fp->_flags & __SWR) == 0 || fp->_bf._base == NULL) {
		fp->_flags |= __SERR;
		return (-1);
	}
	fp->_p = fp->_bf._base;
	fp->_w = fp->_bf._size;
	return (0);
}

__attribute__((weak)) int
__fflush(FILE *fp)
{
	(void)fp;
	return (0);
}

__attribute__((weak)) int
__sflush(FILE *fp)
{
	(void)fp;
	return (0);
}

__attribute__((weak)) int
__sflags(const char *mode, int *optr)
{
	(void)mode;
	*optr = __SWR;
	return (0);
}

/* The oracle runs round-to-nearest; the donor's Honor_FLT_ROUNDS calls this. */
__attribute__((weak)) int
fegetround(void)
{
	return (0);
}

/* A string stream copies what fits and pretends it wrote everything. */
__attribute__((weak)) int
__sfvwrite(FILE *fp, struct __suio *uio)
{
	struct __siov *iov;
	size_t i;

	if (uio->uio_resid == 0)
		return (0);
	iov = uio->uio_iov;
	for (i = 0; i < (size_t)uio->uio_iovcnt; i++) {
		const unsigned char *p = (const unsigned char *)iov[i].iov_base;
		size_t len = iov[i].iov_len;
		size_t w = (size_t)(fp->_w > 0 ? fp->_w : 0);
		if (len < w)
			w = len;
		if (w != 0) {
			memcpy(fp->_p, p, w);
			fp->_p += w;
			fp->_w -= (int)w;
		}
		uio->uio_resid -= (int)len;
	}
	uio->uio_iovcnt = 0;
	uio->uio_resid = 0;
	return (0);
}

/* Only used by the non-string branch of vfwprintf's __xfputwc. */
__attribute__((weak)) wint_t
__fputwc(wchar_t wc, FILE *fp, locale_t locale)
{
	(void)wc;
	(void)fp;
	(void)locale;
	return ((wint_t)-1);
}

/* Cancellation cleanup hooks are no-ops in the single-threaded oracle. */
struct _pthread_cleanup_info {
	int dummy;
};

static inline void
___pthread_cleanup_push_imp(void (*routine)(void *), void *arg,
    struct _pthread_cleanup_info *info)
{
	(void)routine;
	(void)arg;
	(void)info;
}

static inline void
___pthread_cleanup_pop_imp(int execute)
{
	(void)execute;
}

__attribute__((weak)) int
_FLOCKFILE(FILE *fp)
{
	(void)fp;
	return (0);
}

__attribute__((weak)) void
__stdio_cancel_cleanup(void *arg)
{
	(void)arg;
}

__attribute__((weak)) int
__xvprintf(FILE *fp, const char *fmt, va_list ap)
{
	(void)fp;
	(void)fmt;
	(void)ap;
	return (-1);
}

#endif /* RL_FMT_STDIO_H */

#endif
