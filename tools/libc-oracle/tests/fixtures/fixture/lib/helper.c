// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// First-party fixture source for libc-oracle tests: a cross-file helper.

#include <stddef.h>

size_t fixture_helper(size_t n)
{
	return n + 1;
}
