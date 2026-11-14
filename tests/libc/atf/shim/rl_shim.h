// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

// First-party compatibility macros for compiling donor test C. Macros only: a missing host
// function must make the oracle build fail, which is the correct classification (16 3.B.2.3).

#ifndef RL_SHIM_H
#define RL_SHIM_H

#ifndef __arraycount
#define __arraycount(a) (sizeof(a) / sizeof((a)[0]))
#endif

#ifndef __unused
#define __unused __attribute__((__unused__))
#endif

#ifndef __used
#define __used __attribute__((__used__))
#endif

#ifndef __dead
#define __dead __attribute__((__noreturn__))
#endif

#ifndef __printflike
#define __printflike(a, b) __attribute__((__format__(__printf__, a, b)))
#endif

/* NetBSD's sys/cdefs.h provides these; they carry no code. */
#ifndef __RCSID
#define __RCSID(text)
#endif

#ifndef __COPYRIGHT
#define __COPYRIGHT(text)
#endif

#ifndef __UNCONST
#define __UNCONST(value) ((void *)(unsigned long)(const void *)(value))
#endif

#ifndef __STRING
#define __STRING(value) #value
#endif

#ifndef __CTASSERT
#define __CTASSERT(condition) _Static_assert(condition, #condition)
#endif

#ifndef __BEGIN_DECLS
#ifdef __cplusplus
#define __BEGIN_DECLS extern "C" {
#define __END_DECLS }
#else
#define __BEGIN_DECLS
#define __END_DECLS
#endif
#endif

#endif /* !defined(RL_SHIM_H) */
