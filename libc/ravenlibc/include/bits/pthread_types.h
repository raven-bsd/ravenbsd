/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * S746: the public thread scalar types are the FreeBSD 133344405
 * sys/sys/_pthreadtypes.h spellings: a signed int key and a pointer to the
 * opaque spinlock. The musl bits/alltypes.h spellings are not used for these
 * two names.
 */
#ifndef _RAVEN_BITS_PTHREAD_TYPES_H
#define _RAVEN_BITS_PTHREAD_TYPES_H

#ifndef __DEFINED_pthread_key_t
typedef int pthread_key_t;
#define __DEFINED_pthread_key_t
#endif

struct pthread_spinlock;

#ifndef __DEFINED_pthread_spinlock_t
typedef struct pthread_spinlock *pthread_spinlock_t;
#define __DEFINED_pthread_spinlock_t
#endif

#endif
