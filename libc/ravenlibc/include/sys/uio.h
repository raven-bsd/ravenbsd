/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/uio.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _SYS_UIO_H
#define _SYS_UIO_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_size_t
#define __NEED_ssize_t
#define __NEED_struct_iovec

#if __BSD_VISIBLE
#define __NEED_off_t
#endif

#if __BSD_VISIBLE
#define __NEED_pid_t
#endif

#include <bits/alltypes.h>

#define UIO_MAXIOV 1024

ssize_t readv (int, const struct iovec *, int);
ssize_t writev (int, const struct iovec *, int);

#if __BSD_VISIBLE
ssize_t preadv (int, const struct iovec *, int, off_t);
ssize_t pwritev (int, const struct iovec *, int, off_t);
#if defined(_LARGEFILE64_SOURCE)
#define preadv64 preadv
#define pwritev64 pwritev
#define off64_t off_t
#endif
#endif

#if __BSD_VISIBLE
ssize_t preadv2 (int, const struct iovec *, int, off_t, int);
ssize_t pwritev2 (int, const struct iovec *, int, off_t, int);
#define RWF_HIPRI 0x00000001
#define RWF_DSYNC 0x00000002
#define RWF_SYNC 0x00000004
#define RWF_NOWAIT 0x00000008
#define RWF_APPEND 0x00000010
#define RWF_NOAPPEND 0x00000020
#endif

__END_DECLS

#endif
