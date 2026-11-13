/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/sem.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _SYS_SEM_H
#define _SYS_SEM_H
#include <sys/cdefs.h>
__BEGIN_DECLS

#define __NEED_size_t
#define __NEED_pid_t
#define __NEED_time_t
#if __BSD_VISIBLE
#define __NEED_struct_timespec
#endif
#include <bits/alltypes.h>

#include <sys/ipc.h>

#define SEM_UNDO	0x1000
#define GETPID		11
#define GETVAL		12
#define GETALL		13
#define GETNCNT		14
#define GETZCNT		15
#define SETVAL		16
#define SETALL		17

#include <bits/sem.h>

#define _SEM_SEMUN_UNDEFINED 1

#define SEM_STAT (18 | (IPC_STAT & 0x100))
#define SEM_INFO 19
#define SEM_STAT_ANY (20 | (IPC_STAT & 0x100))

struct  seminfo {
	int semmap;
	int semmni;
	int semmns;
	int semmnu;
	int semmsl;
	int semopm;
	int semume;
	int semusz;
	int semvmx;
	int semaem;
};

struct sembuf {
	unsigned short sem_num;
	short sem_op;
	short sem_flg;
};

int semctl(int, int, int, ...);
int semget(key_t, int, int);
int semop(int, struct sembuf *, size_t);

#if __BSD_VISIBLE
int semtimedop(int, struct sembuf *, size_t, const struct timespec *);
#endif

__END_DECLS
#endif
