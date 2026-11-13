/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/aio.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _AIO_H
#define _AIO_H
#include <sys/cdefs.h>

__BEGIN_DECLS
#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE

#include <signal.h>
#include <time.h>

#define __NEED_ssize_t
#define __NEED_off_t

#include <bits/alltypes.h>

struct aiocb {
	int aio_fildes, aio_lio_opcode, aio_reqprio;
	volatile void *aio_buf;
	size_t aio_nbytes;
	struct sigevent aio_sigevent;
	void *__td;
	int __lock[2];
	volatile int __err;
	ssize_t __ret;
	off_t aio_offset;
	void *__next, *__prev;
	char __dummy4[32-2*sizeof(void *)];
};

#define AIO_CANCELED 0
#define AIO_NOTCANCELED 1
#define AIO_ALLDONE 2

#define LIO_READ 0
#define LIO_WRITE 1
#define LIO_NOP 2

#define LIO_WAIT 0
#define LIO_NOWAIT 1

int aio_read(struct aiocb *);
int aio_write(struct aiocb *);
int aio_error(const struct aiocb *);
ssize_t aio_return(struct aiocb *);
int aio_cancel(int, struct aiocb *);
int aio_suspend(const struct aiocb *const [], int, const struct timespec *);
int aio_fsync(int, struct aiocb *);

int lio_listio(int, struct aiocb *__restrict const *__restrict, int, struct sigevent *__restrict);

#endif
__END_DECLS

#endif
