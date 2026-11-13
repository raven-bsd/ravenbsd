/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <sys/epoll.h>

extern "C" int epoll_create(int size)
{
	return size;
}
