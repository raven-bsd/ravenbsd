/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/shm.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _SYS_SHM_H
#define _SYS_SHM_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_time_t
#define __NEED_size_t
#define __NEED_pid_t

#include <bits/alltypes.h>

#include <sys/ipc.h>

#if __BSD_VISIBLE
#define __used_ids used_ids
#define __swap_attempts swap_attempts
#define __swap_successes swap_successes
#endif

#pragma push_macro("__unused")
#undef __unused
#include <bits/shm.h>
#pragma pop_macro("__unused")

#define SHM_R 0400
#define SHM_W 0200

#define SHM_RDONLY 010000
#define SHM_RND    020000
#define SHM_REMAP  040000
#define SHM_EXEC   0100000

#define SHM_LOCK 11
#define SHM_UNLOCK 12
#define SHM_STAT (13 | (IPC_STAT & 0x100))
#define SHM_INFO 14
#define SHM_STAT_ANY (15 | (IPC_STAT & 0x100))
#define SHM_DEST 01000
#define SHM_LOCKED 02000
#define SHM_HUGETLB 04000
#define SHM_NORESERVE 010000

#define SHM_HUGE_SHIFT 26
#define SHM_HUGE_MASK  0x3f
#define SHM_HUGE_64KB  (16 << 26)
#define SHM_HUGE_512KB (19 << 26)
#define SHM_HUGE_1MB   (20 << 26)
#define SHM_HUGE_2MB   (21 << 26)
#define SHM_HUGE_8MB   (23 << 26)
#define SHM_HUGE_16MB  (24 << 26)
#define SHM_HUGE_32MB  (25 << 26)
#define SHM_HUGE_256MB (28 << 26)
#define SHM_HUGE_512MB (29 << 26)
#define SHM_HUGE_1GB   (30 << 26)
#define SHM_HUGE_2GB   (31 << 26)
#define SHM_HUGE_16GB  (34U << 26)

typedef unsigned long shmatt_t;

void *shmat(int, const void *, int);
int shmctl(int, int, struct shmid_ds *);
int shmdt(const void *);
int shmget(key_t, size_t, int);

__END_DECLS

#endif
