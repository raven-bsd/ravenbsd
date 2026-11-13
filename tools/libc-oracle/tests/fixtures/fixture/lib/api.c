// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// First-party fixture source for libc-oracle tests: calls the helper and memcpy.

#include <stddef.h>

void *memcpy(void *dst, const void *src, size_t n);
size_t fixture_helper(size_t n);

size_t fixture_api(const void *p, size_t n)
{
	char buf[64];

	memcpy(buf, p, n);
	return fixture_helper(n) + (unsigned char)buf[0];
}
