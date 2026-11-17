/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * C-ABI smoke test for the `long double` exports: the host x86-64 SysV ABI passes the 80-bit
 * value in a 16-byte memory slot and returns it in `st(0)`, which is the same mapping the target
 * documents (`va/README.md`, RD-33). The program has its own `_start` so it can link the archive
 * without a host C library.
 */

extern long double acosl(long double);
extern long double cbrtl(long double);
extern long double ceill(long double);
extern long double copysignl(long double, long double);
extern long double fabsl(long double);
extern long double fdiml(long double, long double);
extern long double floorl(long double);
extern long double fmal(long double, long double, long double);
extern long double fmaxl(long double, long double);
extern long double fminl(long double, long double);
extern long double fmodl(long double, long double);
extern long double frexpl(long double, int *);
extern long double hypotl(long double, long double);
extern int ilogbl(long double);
extern long double ldexpl(long double, int);
extern long double lgammal(long double);
extern long double lgammal_r(long double, int *);
extern long llogbl(long double);
extern long double logbl(long double);
extern long double modfl(long double, long double *);
extern long double nanl(const char *);
extern long double nearbyintl(long double);
extern long double nextafterl(long double, long double);
extern long double pow10l(long double);
extern long double powl(long double, long double);
extern long double remainderl(long double, long double);
extern long double remquol(long double, long double, int *);
extern long double rintl(long double);
extern long double roundl(long double);
extern long double scalblnl(long double, long);
extern long double scalbnl(long double, int);
extern long double significandl(long double);
extern void sincosl(long double, long double *, long double *);
extern long double sqrtl(long double);
extern long double truncl(long double);

static int near(long double a, long double b)
{
	long double d = a - b;
	if (d < 0)
		d = -d;
	if (b < 0)
		b = -b;
	return d <= 1e-15L * (b > 1.0L ? b : 1.0L);
}

int main(void)
{
	long double sin_value, cos_value, fraction, integral, value;
	int exponent = 0;
	int sign = 0;
	int quotient = 0;

	if (!near(acosl(-1.0L), 3.14159265358979323846L))
		return 10;
	if (!near(cbrtl(27.0L), 3.0L))
		return 11;
	if (ceill(2.1L) != 3.0L || floorl(2.9L) != 2.0L || truncl(-2.9L) != -2.0L)
		return 12;
	if (roundl(2.5L) != 3.0L || rintl(2.5L) != 2.0L || nearbyintl(2.5L) != 2.0L)
		return 13;
	if (fabsl(-2.0L) != 2.0L || copysignl(2.0L, -1.0L) != -2.0L)
		return 14;
	if (fdiml(3.0L, 1.0L) != 2.0L || fmaxl(2.0L, 3.0L) != 3.0L || fminl(2.0L, 3.0L) != 2.0L)
		return 15;
	if (fmodl(5.5L, 2.0L) != 1.5L || !near(remainderl(5.5L, 2.0L), -0.5L))
		return 16;
	if (fmal(2.0L, 3.0L, 4.0L) != 10.0L)
		return 17;
	if (frexpl(48.0L, &exponent) != 0.75L || exponent != 6)
		return 18;
	if (!near(hypotl(3.0L, 4.0L), 5.0L))
		return 19;
	if (ilogbl(8.0L) != 3 || !near(logbl(8.0L), 3.0L) || llogbl(0.5L) != -1)
		return 20;
	if (ldexpl(1.5L, 3) != 12.0L || scalbnl(1.5L, 3) != 12.0L || scalblnl(1.5L, -2) != 0.375L)
		return 21;
	if (!near(lgammal(5.0L), 3.17805383034794561965L))
		return 22;
	if (!near(lgammal_r(5.0L, &sign), 3.17805383034794561965L) || sign != 1)
		return 23;
	fraction = modfl(2.75L, &integral);
	if (fraction != 0.75L || integral != 2.0L)
		return 24;
	if (!(nanl("1") != nanl("1")))
		return 25;
	if (!near(pow10l(3.0L), 1000.0L) || !near(powl(2.0L, 10.0L), 1024.0L))
		return 26;
	value = remquol(5.5L, 2.0L, &quotient);
	if (!near(value, -0.5L) || quotient != 3)
		return 27;
	if (significandl(8.0L) != 1.0L)
		return 28;
	sincosl(0.0L, &sin_value, &cos_value);
	if (sin_value != 0.0L || cos_value != 1.0L)
		return 29;
	if (!near(sqrtl(9.0L), 3.0L))
		return 30;
	if (nextafterl(1.0L, 2.0L) <= 1.0L)
		return 31;
	return 0;
}

__attribute__((noreturn)) void _start(void)
{
	int code = main();
	__asm__ volatile("syscall" :: "a"(60), "D"((long)code) : "rcx", "r11", "memory");
	__builtin_unreachable();
}
