/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * CLEANROOM: architecture/raven-libc/06-solaris-design.md A-sol-5 and B-sol-11
 * (ticket C67 5.E). The Solaris thread spellings are ordinary exported functions over
 * the POSIX threads of <pthread.h>; there is no second thread implementation.
 */

#ifndef	_THREAD_H_
#define	_THREAD_H_

#include <sys/cdefs.h>
#include <pthread.h>
#include <signal.h>
#include <time.h>

#ifndef	__SOLARIS_VISIBLE
#define	__SOLARIS_VISIBLE	__BSD_VISIBLE
#endif

__BEGIN_DECLS

#if __SOLARIS_VISIBLE

typedef pthread_t		thread_t;
typedef pthread_mutex_t		mutex_t;
typedef pthread_cond_t		cond_t;
typedef pthread_rwlock_t	rwlock_t;
typedef pthread_key_t		thread_key_t;
typedef struct timespec		timestruc_t;

#define	THR_BOUND	0x1	/* ours */
#define	THR_NEW_LWP	0x2	/* ours */
#define	THR_DETACHED	0x4	/* ours */
#define	THR_SUSPENDED	0x8	/* ours */
#define	THR_DAEMON	0x10	/* ours */

#define	USYNC_THREAD	0	/* ours */
#define	USYNC_PROCESS	1	/* ours */

int thr_create(void *, size_t, void *(*)(void *), void *, long, thread_t *);
void thr_exit(void *);
int thr_join(thread_t, thread_t *, void **);
thread_t thr_self(void);
void thr_yield(void);
int thr_kill(thread_t, int);
int thr_sigsetmask(int, const sigset_t *, sigset_t *);
int thr_keycreate(thread_key_t *, void (*)(void *));
int thr_setspecific(thread_key_t, void *);
int thr_getspecific(thread_key_t, void **);
size_t thr_min_stack(void);
int thr_main(void);
int thr_getprio(thread_t, int *);
int thr_setprio(thread_t, int);
int thr_setconcurrency(int);
int thr_getconcurrency(void);
int thr_suspend(thread_t);
int thr_continue(thread_t);

#endif /* __SOLARIS_VISIBLE */

__END_DECLS

#endif /* _THREAD_H_ */
