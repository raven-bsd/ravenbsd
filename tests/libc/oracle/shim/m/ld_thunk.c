/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RD-33 thunk: `long double` is passed by hidden pointer (the x87 stack slot / 16-byte memory
 * object), so the oracle takes and returns the 16-byte image instead of a by-value argument.
 */
#include <string.h>

long double expl(long double);

void rl_expl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = expl(x);
	memcpy(out, &y, sizeof(y));
}
