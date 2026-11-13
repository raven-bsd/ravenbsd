/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/values.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef _VALUES_H
#define _VALUES_H
#include <sys/cdefs.h>

#include <limits.h>

#define CHARBITS   (sizeof(char)   * 8)
#define SHORTBITS  (sizeof(short)  * 8)
#define INTBITS    (sizeof(int)    * 8)
#define LONGBITS   (sizeof(long)   * 8)
#define PTRBITS    (sizeof(char *) * 8)
#define DOUBLEBITS (sizeof(double) * 8)
#define FLOATBITS  (sizeof(float)  * 8)

#define MINSHORT SHRT_MIN
#define MININT   INT_MIN
#define MINLONG  LONG_MIN

#define MAXSHORT SHRT_MAX
#define MAXINT   INT_MAX
#define MAXLONG  LONG_MAX

#define HIBITS   MINSHORT
#define HIBITL   MINLONG

#include <float.h>

#define MAXDOUBLE DBL_MAX
#undef  MAXFLOAT
#define MAXFLOAT  FLT_MAX
#define MINDOUBLE DBL_MIN
#define MINFLOAT  FLT_MIN
#define DMINEXP   DBL_MIN_EXP
#define FMINEXP   FLT_MIN_EXP
#define DMAXEXP   DBL_MAX_EXP
#define FMAXEXP   FLT_MAX_EXP

#define BITSPERBYTE CHAR_BIT

#endif
