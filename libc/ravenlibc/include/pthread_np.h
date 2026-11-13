/*-
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * The BSD `_np` thread interface. The names are the B09 row of
 * architecture/raven-libc/02-api-surface.md; the text is RavenBSD's own.
 * `pthread_set_name_np` and `pthread_get_name_np` alias `pthread_setname_np` and
 * `pthread_getname_np`; `pthread_attr_get_np` aliases `pthread_getattr_np`; suspend and resume
 * are `ENOSYS` until a kernel primitive exists.
 */

#ifndef _PTHREAD_NP_H_
#define _PTHREAD_NP_H_

#include <sys/cdefs.h>
#include <pthread.h>
#include <time.h>

__BEGIN_DECLS

/* The BSD spelling of pthread_setname_np; the name is limited to 15 bytes plus NUL. */
void pthread_set_name_np(pthread_t, const char *);

/* The BSD spelling of pthread_getname_np; the buffer receives a NUL-terminated name. */
void pthread_get_name_np(pthread_t, char *, size_t);

/* The BSD spelling of pthread_getattr_np. */
int pthread_attr_get_np(pthread_t, pthread_attr_t *);

/* Non-zero when the calling thread is the initial thread of the process. */
int pthread_main_np(void);

/* Suspend and resume are not implemented (ENOSYS); the signatures are frozen for source
 * compatibility with BSD programs. */
int pthread_suspend_np(pthread_t);
int pthread_resume_np(pthread_t);

/* The BSD spelling of sched_yield. */
int pthread_yield(void);

__END_DECLS

#endif /* _PTHREAD_NP_H_ */
