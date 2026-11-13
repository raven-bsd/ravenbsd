/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the gdtoa build used here is not compiled with MULTIPLE_THREADS (the Rust
 * port replaces both locks with the per-call arena), so these stand-ins only have to satisfy the
 * preprocessor if a recipe ever defines _REENTRANT.  Tests that call the C original from several
 * threads serialize those calls with a Rust mutex.
 */
#ifndef _ORACLE_GDTOA_REENTRANT_H
#define _ORACLE_GDTOA_REENTRANT_H
typedef int mutex_t;
#define MUTEX_INITIALIZER 0
#define mutex_lock(m) ((void)(m))
#define mutex_unlock(m) ((void)(m))
#endif
