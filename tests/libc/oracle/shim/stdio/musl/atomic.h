/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <atomic.h> for the musl stdio oracle: the compiler's atomic builtins stand in
 * for the arch asm of the donor.  The oracle is single-threaded; the operations still provide
 * the acquire/release ordering the lock code relies on.
 */
#ifndef RL_STDIO_MUSL_ATOMIC_H
#define RL_STDIO_MUSL_ATOMIC_H

static inline int a_cas(volatile int *p, int t, int s)
{
	int expected = t;
	__atomic_compare_exchange_n(p, &expected, s, 0,
				    __ATOMIC_ACQ_REL, __ATOMIC_RELAXED);
	return expected;
}

static inline int a_swap(volatile int *p, int v)
{
	return __atomic_exchange_n(p, v, __ATOMIC_ACQ_REL);
}

static inline void a_store(volatile int *p, int v)
{
	__atomic_store_n(p, v, __ATOMIC_RELEASE);
}

static inline int a_fetch_add(volatile int *p, int v)
{
	return __atomic_fetch_add(p, v, __ATOMIC_ACQ_REL);
}

#endif
