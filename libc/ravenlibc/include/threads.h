/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/threads.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _THREADS_H
#define _THREADS_H

#include <sys/cdefs.h>
#include <time.h>

__BEGIN_DECLS

#ifdef __cplusplus
typedef unsigned long thrd_t;
#else
typedef struct __pthread *thrd_t;
#define thread_local _Thread_local
#endif

typedef int once_flag;
typedef unsigned tss_t;
typedef int (*thrd_start_t)(void *);
typedef void (*tss_dtor_t)(void *);

#define __NEED_cnd_t
#define __NEED_mtx_t

#include <bits/alltypes.h>

#define TSS_DTOR_ITERATIONS 4

enum {
	thrd_success  = 0,
	thrd_busy     = 1,
	thrd_error    = 2,
	thrd_nomem    = 3,
	thrd_timedout = 4,
};

enum {
	mtx_plain     = 0,
	mtx_recursive = 1,
	mtx_timed     = 2,
};

#define ONCE_FLAG_INIT 0

int thrd_create(thrd_t *, thrd_start_t, void *);
__dead2 void thrd_exit(int);

int thrd_detach(thrd_t);
int thrd_join(thrd_t, int *);

int thrd_sleep(const struct timespec *, struct timespec *);
void thrd_yield(void);

thrd_t thrd_current(void);
int thrd_equal(thrd_t, thrd_t);
#ifndef __cplusplus
#define thrd_equal(A, B) ((A) == (B))
#endif

void call_once(once_flag *, void (*)(void));

int mtx_init(mtx_t *, int);
void mtx_destroy(mtx_t *);

int mtx_lock(mtx_t *);
int mtx_timedlock(mtx_t *__restrict, const struct timespec *__restrict);
int mtx_trylock(mtx_t *);
int mtx_unlock(mtx_t *);

int cnd_init(cnd_t *);
void cnd_destroy(cnd_t *);

int cnd_broadcast(cnd_t *);
int cnd_signal(cnd_t *);

int cnd_timedwait(cnd_t *__restrict, mtx_t *__restrict, const struct timespec *__restrict);
int cnd_wait(cnd_t *, mtx_t *);

int tss_create(tss_t *, tss_dtor_t);
void tss_delete(tss_t);

int tss_set(tss_t, void *);
void *tss_get(tss_t);

__END_DECLS

#endif
