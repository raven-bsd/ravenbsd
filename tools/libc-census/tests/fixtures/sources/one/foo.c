/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <sys/queue.h>
#include <stdio.h>

int fixture_one(const char *src)
{
	return (int)strlcpy(0, src, 0) + (int)memchr(src, 'x', 1);
}
