/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * CLEANROOM: architecture/raven-libc/06-solaris-design.md A-sol-47 (ticket C67 5.E).
 * Event ports over the Raven event queue. The numeric values are Raven values; source
 * compatibility only (C67 4.3).
 */

#ifndef	_PORT_H_
#define	_PORT_H_

#include <sys/cdefs.h>
#include <sys/types.h>
#include <time.h>
#include <stdint.h>

/*
 * C37's sys/_visible.h does not define __SOLARIS_VISIBLE yet; the headers of this ticket
 * use the BSD visibility bit, which is on by default and cleared by every strict mode
 * (RD-new, reported as a QUESTION).
 */
#ifndef	__SOLARIS_VISIBLE
#define	__SOLARIS_VISIBLE	__BSD_VISIBLE
#endif

__BEGIN_DECLS

#if __SOLARIS_VISIBLE

typedef struct timespec timespec_t;

#define	PORT_SOURCE_TIMER	1	/* donor-map/14 3.B.3.e */
#define	PORT_SOURCE_USER	3	/* ours */
#define	PORT_SOURCE_FD		4	/* donor-map/14 3.B.3.e */
#define	PORT_SOURCE_ALERT	5	/* ours, reserved: not delivered */

typedef struct port_event {
	int		portev_events;
	unsigned short	portev_source;
	unsigned short	portev_pad;
	uintptr_t	portev_object;
	void		*portev_user;
} port_event_t;

#if defined(__cplusplus)
static_assert(sizeof(port_event_t) == 24, "port_event_t is 24 bytes on LP64");
#else
_Static_assert(sizeof(port_event_t) == 24, "port_event_t is 24 bytes on LP64");
#endif

int port_create(void);
int port_associate(int, int, uintptr_t, int, void *);
int port_dissociate(int, int, uintptr_t);
int port_get(int, port_event_t *, timespec_t *);
int port_getn(int, port_event_t [], unsigned int, unsigned int *, timespec_t *);
int port_send(int, int, void *);
int port_alert(int, int, int, void *);

#endif /* __SOLARIS_VISIBLE */

__END_DECLS

#endif /* _PORT_H_ */
