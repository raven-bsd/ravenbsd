/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * CLEANROOM: architecture/raven-libc/06-solaris-design.md A-sol-5 and A-sol-15
 * (ticket C67 5.E). The Solaris synchronisation spellings forward to the POSIX calls;
 * USYNC_THREAD/USYNC_PROCESS are the "ours" values of C67 4.3.
 */

#ifndef	_SYNCH_H_
#define	_SYNCH_H_

#include <sys/cdefs.h>
#include <thread.h>
#include <semaphore.h>

#ifndef	__SOLARIS_VISIBLE
#define	__SOLARIS_VISIBLE	__BSD_VISIBLE
#endif

__BEGIN_DECLS

#if __SOLARIS_VISIBLE

typedef sem_t sema_t;

int mutex_init(mutex_t *, int, void *);
int mutex_destroy(mutex_t *);
int mutex_lock(mutex_t *);
int mutex_trylock(mutex_t *);
int mutex_unlock(mutex_t *);

int cond_init(cond_t *, int, void *);
int cond_destroy(cond_t *);
int cond_wait(cond_t *, mutex_t *);
int cond_timedwait(cond_t *, mutex_t *, timestruc_t *);
int cond_signal(cond_t *);
int cond_broadcast(cond_t *);

int rwlock_init(rwlock_t *, int, void *);
int rwlock_destroy(rwlock_t *);
int rw_rdlock(rwlock_t *);
int rw_wrlock(rwlock_t *);
int rw_tryrdlock(rwlock_t *);
int rw_trywrlock(rwlock_t *);
int rw_unlock(rwlock_t *);

int sema_init(sema_t *, unsigned int, int, void *);
int sema_destroy(sema_t *);
int sema_wait(sema_t *);
int sema_trywait(sema_t *);
int sema_post(sema_t *);

#endif /* __SOLARIS_VISIBLE */

__END_DECLS

#endif /* _SYNCH_H_ */
