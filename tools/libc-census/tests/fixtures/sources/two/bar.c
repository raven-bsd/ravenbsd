/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <sys/queue.h>

int fixture_two(void)
{
	return epoll_create(1);
}
