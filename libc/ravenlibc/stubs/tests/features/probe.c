/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * The feature-macro acceptance probe of ticket C66 5.E: every accepted macro must keep the
 * Linux event API, the BSD event API, GNU error() and the BSD strings visible at once.
 */
#include <sys/epoll.h>
#include <sys/event.h>
#include <error.h>
#include <string.h>

int raven_c66_probe(void)
{
	char buffer[8];
	int epfd = epoll_create1(EPOLL_CLOEXEC);
	int kqfd = kqueue();
	error(0, 0, "probe");
	return epfd + kqfd + (int)strlcpy(buffer, "x", sizeof(buffer));
}
