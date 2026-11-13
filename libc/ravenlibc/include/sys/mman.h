/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/mman.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_SYS_MMAN_H
#define	_SYS_MMAN_H
#include <sys/cdefs.h>
#include <bits/abi_mman.h>
__BEGIN_DECLS

#define __NEED_mode_t
#define __NEED_size_t
#define __NEED_off_t

#if __BSD_VISIBLE
#define __NEED_ssize_t
#endif

#include <bits/alltypes.h>

#define MAP_FAILED ((void *) -1)

#if __BSD_VISIBLE
#endif

#if __BSD_VISIBLE

#define MLOCK_ONFAULT 0x01

#endif

#include <bits/mman.h>

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
void *mmap (void *, size_t, int, int, int, off_t);
int munmap (void *, size_t);

int mprotect (void *, size_t, int);
int msync (void *, size_t, int);

int posix_madvise (void *, size_t, int);

int mlock (const void *, size_t);
int munlock (const void *, size_t);
int mlockall (int);
int munlockall (void);

#if __BSD_VISIBLE
void *mremap (void *, size_t, size_t, int, ...);
int memfd_create (const char *, unsigned);
int mlock2 (const void *, size_t, unsigned);
#endif

#if __BSD_VISIBLE
int madvise (void *, size_t, int);
int mincore (void *, size_t, unsigned char *);
#endif

int shm_open (const char *, int, mode_t);
int shm_unlink (const char *);

#endif
__END_DECLS
#endif
