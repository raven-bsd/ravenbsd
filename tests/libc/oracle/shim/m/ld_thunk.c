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

/* The donor acoshl calls sqrtl(); the donor x87 implementation needs <fenv.h>, whose glibc
 * header conflicts with the donor sys/cdefs.h on this host, so the thunk provides the hardware
 * square root instead. */
long double sqrtl(long double x)
{
	long double y;

	__asm__("fsqrt" : "=t"(y) : "0"(x));
	return y;
}

/* The donor argument reduction (k_rem_pio2.c) calls floor(); the oracle .so is not linked
 * against libm, so the first-party shim provides it. */
double floor(double x)
{
	double t;

	if (!(x > -4503599627370496.0 && x < 4503599627370496.0))
		return x;
	t = (double)(long long)x;
	return t > x ? t - 1.0 : t;
}
long double expm1l(long double);
long double logl(long double);
long double log10l(long double);
long double log1pl(long double);
long double log2l(long double);
long double sinl(long double);
long double cosl(long double);
long double tanl(long double);
long double sinhl(long double);
long double coshl(long double);
long double tanhl(long double);
long double asinhl(long double);
long double acoshl(long double);
long double atanhl(long double);
long double erfl(long double);
long double erfcl(long double);
long double lgammal_r(long double, int *);
long double tgammal(long double);
long double ceill(long double);
long double copysignl(long double, long double);
long double fdiml(long double, long double);
long double floorl(long double);
long double fmaxl(long double, long double);
long double fminl(long double, long double);
long double fmodl(long double, long double);
long double nextafterl(long double, long double);
long double nexttowardl(long double, long double);
long double remainderl(long double, long double);
long double remquol(long double, long double, int *);
long double rintl(long double);
long double roundl(long double);
long double scalbnl(long double, int);
long double truncl(long double);

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

void rl_sinl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = sinl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_cosl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = cosl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_tanl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = tanl(x);
	memcpy(out, &y, sizeof(y));
}
void rl_sinhl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = sinhl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_coshl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = coshl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_tanhl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = tanhl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_asinhl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = asinhl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_acoshl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = acoshl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_atanhl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = atanhl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_erfl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = erfl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_erfcl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = erfcl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_tgammal(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = tgammal(x);
	memcpy(out, &y, sizeof(y));
}

void rl_lgammal_r(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;
	int sign;

	memcpy(&x, in, sizeof(x));
	y = lgammal_r(x, &sign);
	memcpy(out, &y, sizeof(y));
}
void rl_ceill(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = ceill(x);
	memcpy(out, &y, sizeof(y));
}

void rl_floorl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = floorl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_rintl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = rintl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_roundl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = roundl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_truncl(const unsigned char *in, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = truncl(x);
	memcpy(out, &y, sizeof(y));
}

void rl_copysignl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = copysignl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_fdiml(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = fdiml(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_fmaxl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = fmaxl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_fminl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = fminl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_fmodl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = fmodl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_nextafterl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = nextafterl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_nexttowardl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = nexttowardl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_remainderl(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = remainderl(x, y);
	memcpy(out, &z, sizeof(z));
}

void rl_remquol(const unsigned char *ina, const unsigned char *inb, unsigned char *out)
{
	long double x;
	long double y;
	long double z;
	int quo;

	memcpy(&x, ina, sizeof(x));
	memcpy(&y, inb, sizeof(y));
	z = remquol(x, y, &quo);
	memcpy(out, &z, sizeof(z));
}

void rl_scalbnl(const unsigned char *in, int n, unsigned char *out)
{
	long double x;
	long double y;

	memcpy(&x, in, sizeof(x));
	y = scalbnl(x, n);
	memcpy(out, &y, sizeof(y));
}
