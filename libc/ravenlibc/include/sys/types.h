/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/types.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_SYS_TYPES_H
#define	_SYS_TYPES_H
#include <sys/cdefs.h>
__BEGIN_DECLS

#define __NEED_ino_t
#define __NEED_dev_t
#define __NEED_uid_t
#define __NEED_gid_t
#define __NEED_mode_t
#define __NEED_nlink_t
#define __NEED_off_t
#define __NEED_pid_t
#define __NEED_size_t
#define __NEED_ssize_t
#define __NEED_time_t
#define __NEED_timer_t
#define __NEED_clockid_t

#define __NEED_blkcnt_t
#define __NEED_fsblkcnt_t
#define __NEED_fsfilcnt_t
#define __NEED_reclen_t

#define __NEED_id_t
#define __NEED_key_t
#define __NEED_clock_t
#define __NEED_suseconds_t
#define __NEED_blksize_t

#define __NEED_pthread_t
#define __NEED_pthread_attr_t
#define __NEED_pthread_mutexattr_t
#define __NEED_pthread_condattr_t
#define __NEED_pthread_rwlockattr_t
#define __NEED_pthread_barrierattr_t
#define __NEED_pthread_mutex_t
#define __NEED_pthread_cond_t
#define __NEED_pthread_rwlock_t
#define __NEED_pthread_barrier_t
#define __NEED_pthread_spinlock_t
#define __NEED_pthread_key_t
#define __NEED_pthread_once_t
#define __NEED_useconds_t

/* RavenBSD: R-H2, BSD aliases follow the BSD visibility class. */
#if __BSD_VISIBLE
#define __NEED_int8_t
#define __NEED_int16_t
#define __NEED_int32_t
#define __NEED_int64_t
#define __NEED_u_int64_t
#define __NEED_register_t
#endif

#include <bits/alltypes.h>

#if __BSD_VISIBLE
typedef unsigned char u_int8_t;
typedef unsigned short u_int16_t;
typedef unsigned u_int32_t;
typedef char *caddr_t;
typedef unsigned char u_char;
typedef unsigned short u_short, ushort;
typedef unsigned u_int, uint;
typedef unsigned long u_long, ulong;
typedef long long quad_t;
typedef unsigned long long u_quad_t;
#include <endian.h>
/* RavenBSD: <sys/select.h> is C38's; fd_set is not pulled in here so that
 * this header stays self-contained until C38 lands. */
#endif

/*
 * RavenBSD: <sys/sysmacros.h> semantics (C38 owns that header). The guard
 * keeps a later include from redefining these macros.
 */
#if __BSD_VISIBLE && !defined(_SYS_SYSMACROS_H)
#define major(x) \
	((unsigned)( (((x)>>31>>1) & 0xfffff000) | (((x)>>8) & 0x00000fff) ))
#define minor(x) \
	((unsigned)( (((x)>>12) & 0xffffff00) | ((x) & 0x000000ff) ))
#define makedev(x,y) ( \
	(((x)&0xfffff000ULL) << 32) | \
	(((x)&0x00000fffULL) << 8) | \
	(((y)&0xffffff00ULL) << 12) | \
	(((y)&0x000000ffULL)) )
#endif

/* RavenBSD: _LARGEFILE64_SOURCE selects the default environment in
 * sys/_visible.h; the 64-bit aliases are the native types here, so the musl
 * macro block is not shipped (R-H2, one place consumes user macros). */

__END_DECLS
#endif