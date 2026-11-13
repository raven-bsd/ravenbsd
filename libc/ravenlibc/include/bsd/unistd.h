/*-
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: libbsd overlay forwarder (donor-map 15 section 4.2); first-party, no donor code.
 */

#ifndef LIBBSD_OVERLAY
#define LIBBSD_OVERLAY 1
#endif

#ifndef _BSD_UNISTD_H_H_
#define _BSD_UNISTD_H_H_

#include <unistd.h>

/*
 * libbsd's argv-overwrite helper. Raven has no setproctitle (doc 15 section 4.2 and doc 10
 * reject the argv trick), so the helper is a no-op the port can call safely.
 */
static __inline void
setproctitle_init(int argc, char *argv[], char *envp[])
{
	(void)argc;
	(void)argv;
	(void)envp;
}


#endif /* !_BSD_UNISTD_H_H_ */
