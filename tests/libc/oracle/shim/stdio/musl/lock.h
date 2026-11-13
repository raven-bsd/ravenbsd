/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement "lock.h" for the musl stdio oracle: one word, one thread, so a spin on the
 * atomic exchange is the complete lock.
 */
#ifndef RL_STDIO_MUSL_LOCK_H
#define RL_STDIO_MUSL_LOCK_H

static inline void __rl_lock(volatile int *word)
{
	while (__atomic_exchange_n(word, 1, __ATOMIC_ACQ_REL))
		;
}

static inline void __rl_unlock(volatile int *word)
{
	__atomic_store_n(word, 0, __ATOMIC_RELEASE);
}

#define LOCK(x) __rl_lock(x)
#define UNLOCK(x) __rl_unlock(x)

#endif
