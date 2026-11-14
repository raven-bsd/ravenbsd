/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Strict-mode probe: under `_POSIX_C_SOURCE=200809L` alone `strlcpy` must NOT be declared;
 * adding `_GNU_SOURCE` back restores the default all-visible environment (ticket C66 5.E).
 */
#include <string.h>

int raven_c66_strict_probe(void)
{
	char buffer[8];
	return (int)strlcpy(buffer, "x", sizeof(buffer));
}
