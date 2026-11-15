/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RD-33 thunk: `long double` is passed by hidden pointer (the x87 stack slot / 16-byte memory
 * object), so the oracle takes and returns the 16-byte image instead of a by-value argument.
 */
#include <string.h>

long double expl(long double);

/* The donor calls the C library fabsl; the oracle .so is not linked against libm, so the
 * first-party shim provides it. */
long double fabsl(long double x)
{
	return x < 0 ? -x : x;
}
long double expm1l(long double);
long double logl(long double);
long double log10l(long double);
long double log1pl(long double);
long double log2l(long double);

void rl_logl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = logl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_log10l(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = log10l(x);
	memcpy(out, &y, sizeof(y));
}

void rl_log1pl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = log1pl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_log2l(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = log2l(x);
	memcpy(out, &y, sizeof(y));
}

void rl_expl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = expl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_expm1l(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = expm1l(x);
	memcpy(out, &y, sizeof(y));
}
