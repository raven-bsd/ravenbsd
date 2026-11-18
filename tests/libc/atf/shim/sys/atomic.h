// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Minimal original `sys/atomic.h` for the NetBSD libc atomic tests. It maps the NetBSD atomic
// helpers onto the compiler's `__atomic` builtins; it is not a copy of the NetBSD header.

#ifndef RL_SYS_ATOMIC_H
#define RL_SYS_ATOMIC_H

#include <stdint.h>

typedef volatile int atomic_t;

#define RL_ATOMIC_ADD(name, type)                                          \
    static inline type name(volatile type *ptr, type value) {              \
        return __atomic_add_fetch(ptr, value, __ATOMIC_SEQ_CST);           \
    }
#define RL_ATOMIC_INC(name, type)                                          \
    static inline type name(volatile type *ptr) {                          \
        return __atomic_add_fetch(ptr, (type)1, __ATOMIC_SEQ_CST);         \
    }
#define RL_ATOMIC_DEC(name, type)                                          \
    static inline type name(volatile type *ptr) {                          \
        return __atomic_sub_fetch(ptr, (type)1, __ATOMIC_SEQ_CST);         \
    }
#define RL_ATOMIC_SWAP(name, type)                                         \
    static inline type name(volatile type *ptr, type value) {              \
        return __atomic_exchange_n(ptr, value, __ATOMIC_SEQ_CST);          \
    }
#define RL_ATOMIC_CAS(name, type)                                          \
    static inline type name(volatile type *ptr, type old, type value) {    \
        type expected = old;                                               \
        (void)__atomic_compare_exchange_n(ptr, &expected, value, 0,        \
                                          __ATOMIC_SEQ_CST,                \
                                          __ATOMIC_SEQ_CST);               \
        return expected;                                                   \
    }
#define RL_ATOMIC_OR(name, type)                                           \
    static inline type name(volatile type *ptr, type value) {              \
        return __atomic_or_fetch(ptr, value, __ATOMIC_SEQ_CST);            \
    }
#define RL_ATOMIC_AND(name, type)                                          \
    static inline type name(volatile type *ptr, type value) {              \
        return __atomic_and_fetch(ptr, value, __ATOMIC_SEQ_CST);           \
    }

RL_ATOMIC_ADD(atomic_add_32, uint32_t)
RL_ATOMIC_INC(atomic_inc_32, uint32_t)
RL_ATOMIC_DEC(atomic_dec_32, uint32_t)
RL_ATOMIC_SWAP(atomic_swap_32, uint32_t)
RL_ATOMIC_CAS(atomic_cas_32, uint32_t)
RL_ATOMIC_OR(atomic_or_32, uint32_t)
RL_ATOMIC_AND(atomic_and_32, uint32_t)

RL_ATOMIC_ADD(atomic_add_uint, unsigned int)
RL_ATOMIC_INC(atomic_inc_uint, unsigned int)
RL_ATOMIC_DEC(atomic_dec_uint, unsigned int)
RL_ATOMIC_SWAP(atomic_swap_uint, unsigned int)
RL_ATOMIC_CAS(atomic_cas_uint, unsigned int)

RL_ATOMIC_ADD(atomic_add_ulong, unsigned long)
RL_ATOMIC_INC(atomic_inc_ulong, unsigned long)
RL_ATOMIC_DEC(atomic_dec_ulong, unsigned long)
RL_ATOMIC_SWAP(atomic_swap_ulong, unsigned long)
RL_ATOMIC_CAS(atomic_cas_ulong, unsigned long)

RL_ATOMIC_ADD(atomic_add_64, uint64_t)
RL_ATOMIC_INC(atomic_inc_64, uint64_t)
RL_ATOMIC_DEC(atomic_dec_64, uint64_t)
RL_ATOMIC_SWAP(atomic_swap_64, uint64_t)
RL_ATOMIC_CAS(atomic_cas_64, uint64_t)

RL_ATOMIC_ADD(atomic_add_int, int)
RL_ATOMIC_INC(atomic_inc_int, int)
RL_ATOMIC_DEC(atomic_dec_int, int)
RL_ATOMIC_SWAP(atomic_swap_int, int)
RL_ATOMIC_CAS(atomic_cas_int, int)

static inline void *atomic_swap_ptr(volatile void **ptr, void *value) {
    return __atomic_exchange_n(ptr, value, __ATOMIC_SEQ_CST);
}

#define membar_sync() __atomic_thread_fence(__ATOMIC_SEQ_CST)
#define membar_enter() __atomic_thread_fence(__ATOMIC_SEQ_CST)
#define membar_exit() __atomic_thread_fence(__ATOMIC_SEQ_CST)
#define membar_producer() __atomic_thread_fence(__ATOMIC_RELEASE)
#define membar_consumer() __atomic_thread_fence(__ATOMIC_ACQUIRE)
#define membar_acquire() __atomic_thread_fence(__ATOMIC_ACQUIRE)
#define membar_release() __atomic_thread_fence(__ATOMIC_RELEASE)

#endif /* RL_SYS_ATOMIC_H */
