// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// First-party fixture source for the ad-hoc build test.

#include <stddef.h>

size_t fixture_single(const char *p)
{
	size_t n = 0;

	while (p[n] != '\0')
		n++;
	return n;
}
