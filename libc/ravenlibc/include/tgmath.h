/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: POSIX.1-2024 <tgmath.h>. Type-generic macros over the ravenlibc <math.h> and
 * <complex.h> declarations. The real-function set below covers the C17 7.25 functions used by
 * the base system; complex generic coverage follows with the complex batch.
 */
#ifndef _RAVENBSD_TGMATH_H
#define _RAVENBSD_TGMATH_H

#include <math.h>
#include <complex.h>

#ifndef __cplusplus

#define acos(x) _Generic((x), float: acosf, long double: acosl, default: acos)(x)
#define asin(x) _Generic((x), float: asinf, long double: asinl, default: asin)(x)
#define atan(x) _Generic((x), float: atanf, long double: atanl, default: atan)(x)
#define atan2(x, y) _Generic((x), float: atan2f, long double: atan2l, default: atan2)((x), (y))
#define cos(x) _Generic((x), float: cosf, long double: cosl, default: cos)(x)
#define sin(x) _Generic((x), float: sinf, long double: sinl, default: sin)(x)
#define tan(x) _Generic((x), float: tanf, long double: tanl, default: tan)(x)
#define acosh(x) _Generic((x), float: acoshf, long double: acoshl, default: acosh)(x)
#define asinh(x) _Generic((x), float: asinhf, long double: asinhl, default: asinh)(x)
#define atanh(x) _Generic((x), float: atanhf, long double: atanhl, default: atanh)(x)
#define cosh(x) _Generic((x), float: coshf, long double: coshl, default: cosh)(x)
#define sinh(x) _Generic((x), float: sinhf, long double: sinhl, default: sinh)(x)
#define tanh(x) _Generic((x), float: tanhf, long double: tanhl, default: tanh)(x)
#define exp(x) _Generic((x), float: expf, long double: expl, default: exp)(x)
#define exp2(x) _Generic((x), float: exp2f, long double: exp2l, default: exp2)(x)
#define expm1(x) _Generic((x), float: expm1f, long double: expm1l, default: expm1)(x)
#define log(x) _Generic((x), float: logf, long double: logl, default: log)(x)
#define log10(x) _Generic((x), float: log10f, long double: log10l, default: log10)(x)
#define log1p(x) _Generic((x), float: log1pf, long double: log1pl, default: log1p)(x)
#define log2(x) _Generic((x), float: log2f, long double: log2l, default: log2)(x)
#define pow(x, y) _Generic((x), float: powf, long double: powl, default: pow)((x), (y))
#define sqrt(x) _Generic((x), float: sqrtf, long double: sqrtl, default: sqrt)(x)
#define cbrt(x) _Generic((x), float: cbrtf, long double: cbrtl, default: cbrt)(x)
#define fabs(x) _Generic((x), float: fabsf, long double: fabsl, default: fabs)(x)
#define hypot(x, y) _Generic((x), float: hypotf, long double: hypotl, default: hypot)((x), (y))
#define fmod(x, y) _Generic((x), float: fmodf, long double: fmodl, default: fmod)((x), (y))
#define remainder(x, y) _Generic((x), float: remainderf, long double: remainderl, default: remainder)((x), (y))
#define copysign(x, y) _Generic((x), float: copysignf, long double: copysignl, default: copysign)((x), (y))
#define nextafter(x, y) _Generic((x), float: nextafterf, long double: nextafterl, default: nextafter)((x), (y))
#define fdim(x, y) _Generic((x), float: fdimf, long double: fdiml, default: fdim)((x), (y))
#define fmax(x, y) _Generic((x), float: fmaxf, long double: fmaxl, default: fmax)((x), (y))
#define fmin(x, y) _Generic((x), float: fminf, long double: fminl, default: fmin)((x), (y))
#define fma(x, y, z) _Generic((x), float: fmaf, long double: fmal, default: fma)((x), (y), (z))
#define ceil(x) _Generic((x), float: ceilf, long double: ceill, default: ceil)(x)
#define floor(x) _Generic((x), float: floorf, long double: floorl, default: floor)(x)
#define trunc(x) _Generic((x), float: truncf, long double: truncl, default: trunc)(x)
#define round(x) _Generic((x), float: roundf, long double: roundl, default: round)(x)

#endif /* __cplusplus */

#endif /* _RAVENBSD_TGMATH_H */
