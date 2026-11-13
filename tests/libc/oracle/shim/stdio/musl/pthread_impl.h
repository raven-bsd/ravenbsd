/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <pthread_impl.h> for the musl stdio oracle, written from scratch.  The oracle
 * has exactly one thread; the FILE lock still runs its real CAS ladder so the traced lock
 * behaviour is the donor's.
 */
#ifndef RL_STDIO_MUSL_PTHREAD_IMPL_H
#define RL_STDIO_MUSL_PTHREAD_IMPL_H

#include "atomic.h"

#define pthread_t struct rl_pthread *

struct rl_pthread {
	int tid;
	void *stdio_locks;
};

static struct rl_pthread __rl_pthread = { 1, 0 };
#define __pthread_self() (&__rl_pthread)

static inline void __futexwait(volatile void *addr, int expected, int private)
{
	(void)addr; (void)expected; (void)private;
}

static inline void __wake(volatile void *addr, int count, int private)
{
	(void)addr; (void)count; (void)private;
}

#endif
