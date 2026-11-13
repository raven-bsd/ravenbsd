/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: a single-threaded mutex stand-in for the random.c locking macros.
 */
#ifndef _ORACLE_REENTRANT_H
#define _ORACLE_REENTRANT_H
typedef int mutex_t;
#define MUTEX_INITIALIZER 0
#define mutex_lock(m) ((void)(m))
#define mutex_unlock(m) ((void)(m))
#endif
