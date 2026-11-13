/*-
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Copyright (c) 1999,2000,2001 Jonathan Lemon <jlemon@FreeBSD.org>
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 *
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from freebsd:sys/sys/event.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
 */

#ifndef _SYS_EVENT_H_
#define _SYS_EVENT_H_

#include <sys/cdefs.h>
#include <stdint.h>
#include <stddef.h>

#define EVFILT_READ (-1)
#define EVFILT_WRITE (-2)
#define EVFILT_AIO (-3)
#define EVFILT_VNODE (-4)
#define EVFILT_PROC (-5)
#define EVFILT_SIGNAL (-6)
#define EVFILT_TIMER (-7)
#define EVFILT_PROCDESC (-8)
#define EVFILT_FS (-9)
#define EVFILT_LIO (-10)
#define EVFILT_USER (-11)
#define EVFILT_SENDFILE (-12)
#define EVFILT_EMPTY (-13)
#define EVFILT_JAIL (-14)
#define EVFILT_JAILDESC (-15)
#define EVFILT_SYSCOUNT 15

#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 199901L
#define EV_SET(kevp_, a, b, c, d, e, f) do { \
	*(kevp_) = (struct kevent){ \
	    .ident = (a), \
	    .filter = (b), \
	    .flags = (c), \
	    .fflags = (d), \
	    .data = (e), \
	    .udata = (f), \
	    .ext = {0}, \
	}; \
} while (0)
#else
#define EV_SET(kevp_, a, b, c, d, e, f) do { \
	struct kevent *kevp = (kevp_); \
	(kevp)->ident = (a); \
	(kevp)->filter = (b); \
	(kevp)->flags = (c); \
	(kevp)->fflags = (d); \
	(kevp)->data = (e); \
	(kevp)->udata = (f); \
	(kevp)->ext[0] = 0; \
	(kevp)->ext[1] = 0; \
	(kevp)->ext[2] = 0; \
	(kevp)->ext[3] = 0; \
} while (0)
#endif

struct kevent {
	uintptr_t ident;
	short filter;
	unsigned short flags;
	unsigned int fflags;
	int64_t data;
	void *udata;
	uint64_t ext[4];
};

#define EV_ADD 0x0001
#define EV_DELETE 0x0002
#define EV_ENABLE 0x0004
#define EV_DISABLE 0x0008
#define EV_FORCEONESHOT 0x0100
#define EV_KEEPUDATA 0x0200

#define EV_ONESHOT 0x0010
#define EV_CLEAR 0x0020
#define EV_RECEIPT 0x0040
#define EV_DISPATCH 0x0080

#define EV_SYSFLAGS 0xF000
#define EV_DROP 0x1000
#define EV_FLAG1 0x2000
#define EV_FLAG2 0x4000

#define EV_EOF 0x8000
#define EV_ERROR 0x4000

#define NOTE_FFNOP 0x00000000
#define NOTE_FFAND 0x40000000
#define NOTE_FFOR 0x80000000
#define NOTE_FFCOPY 0xc0000000
#define NOTE_FFCTRLMASK 0xc0000000
#define NOTE_FFLAGSMASK 0x00ffffff
#define NOTE_TRIGGER 0x01000000

#define NOTE_LOWAT 0x0001
#define NOTE_FILE_POLL 0x0002

#define NOTE_DELETE 0x0001
#define NOTE_WRITE 0x0002
#define NOTE_EXTEND 0x0004
#define NOTE_ATTRIB 0x0008
#define NOTE_LINK 0x0010
#define NOTE_RENAME 0x0020
#define NOTE_REVOKE 0x0040
#define NOTE_OPEN 0x0080
#define NOTE_CLOSE 0x0100
#define NOTE_CLOSE_WRITE 0x0200
#define NOTE_READ 0x0400

#define NOTE_EXIT 0x80000000
#define NOTE_FORK 0x40000000
#define NOTE_EXEC 0x20000000
#define NOTE_PDSIGCHLD 0x10000000
#define NOTE_SIGNAL 0x08000000
#define NOTE_REAP 0x04000000
#define NOTE_PCTRLMASK 0xfc000000
#define NOTE_PDATAMASK 0x000fffff

#define NOTE_TRACK 0x00000001
#define NOTE_TRACKERR 0x00000002
#define NOTE_CHILD 0x00000004

#define NOTE_JAIL_CHILD 0x80000000
#define NOTE_JAIL_SET 0x40000000
#define NOTE_JAIL_ATTACH 0x20000000
#define NOTE_JAIL_REMOVE 0x10000000
#define NOTE_JAIL_MULTI 0x08000000
#define NOTE_JAIL_CTRLMASK 0xf0000000

#define NOTE_SECONDS 0x00000001
#define NOTE_MSECONDS 0x00000002
#define NOTE_USECONDS 0x00000004
#define NOTE_NSECONDS 0x00000008
#define NOTE_ABSTIME 0x00000010

#define KQUEUE_CLOEXEC 0x00000001
#define KQUEUE_CPONFORK 0x00000002

struct timespec;

__BEGIN_DECLS
#if __BSD_VISIBLE
int kqueue(void);
int kqueuex(unsigned int);
int kqueue1(int);
int kevent(int, const struct kevent *, int, struct kevent *, int,
    const struct timespec *);
#endif
__END_DECLS

#endif /* _SYS_EVENT_H_ */
