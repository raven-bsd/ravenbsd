/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/ctype.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _CTYPE_H
#define _CTYPE_H

#include <sys/cdefs.h>

#define __NEED_locale_t
#include <bits/alltypes.h>

__BEGIN_DECLS

/*
 * RavenBSD: both forms exist (doc 02 O-10): the exported symbols are real functions so their
 * address can be taken, and the `static __inline` helpers below are the compile-time forms.
 * There is no `_ctype_` table and no `__ctype_b_loc` (A-ctype-1).
 */
int isalnum(int);
int isalpha(int);
int isblank(int);
int iscntrl(int);
int isdigit(int);
int isgraph(int);
int islower(int);
int isprint(int);
int ispunct(int);
int isspace(int);
int isupper(int);
int isxdigit(int);
int tolower(int);
int toupper(int);
int isascii(int);
int toascii(int);

int isalnum_l(int, locale_t);
int isalpha_l(int, locale_t);
int isblank_l(int, locale_t);
int iscntrl_l(int, locale_t);
int isdigit_l(int, locale_t);
int isgraph_l(int, locale_t);
int islower_l(int, locale_t);
int isprint_l(int, locale_t);
int ispunct_l(int, locale_t);
int isspace_l(int, locale_t);
int isupper_l(int, locale_t);
int isxdigit_l(int, locale_t);
int tolower_l(int, locale_t);
int toupper_l(int, locale_t);

#ifndef __cplusplus
static __inline int __raven_isascii(int _c)
{
	return (unsigned)_c < 128;
}

static __inline int __raven_toascii(int _c)
{
	return _c & 0x7f;
}

static __inline int __raven_isalpha(int _c)
{
	return (((unsigned)_c | 32) - 'a') < 26;
}

static __inline int __raven_isdigit(int _c)
{
	return ((unsigned)_c - '0') < 10;
}

static __inline int __raven_isalnum(int _c)
{
	return __raven_isalpha(_c) || __raven_isdigit(_c);
}

static __inline int __raven_isblank(int _c)
{
	return _c == ' ' || _c == '\t';
}

static __inline int __raven_iscntrl(int _c)
{
	return (unsigned)_c < 0x20 || _c == 0x7f;
}

static __inline int __raven_isgraph(int _c)
{
	return ((unsigned)_c - 0x21) < 0x5e;
}

static __inline int __raven_islower(int _c)
{
	return ((unsigned)_c - 'a') < 26;
}

static __inline int __raven_isprint(int _c)
{
	return ((unsigned)_c - 0x20) < 0x5f;
}

static __inline int __raven_ispunct(int _c)
{
	return __raven_isgraph(_c) && !__raven_isalnum(_c);
}

static __inline int __raven_isspace(int _c)
{
	return _c == ' ' || ((unsigned)_c - '\t') < 5;
}

static __inline int __raven_isupper(int _c)
{
	return ((unsigned)_c - 'A') < 26;
}

static __inline int __raven_isxdigit(int _c)
{
	return __raven_isdigit(_c) || (((unsigned)_c | 32) - 'a') < 6;
}

static __inline int __raven_tolower(int _c)
{
	return __raven_isupper(_c) ? _c | 32 : _c;
}

static __inline int __raven_toupper(int _c)
{
	return __raven_islower(_c) ? _c & 0x5f : _c;
}

#define isalnum(a) __raven_isalnum(a)
#define isalpha(a) __raven_isalpha(a)
#define isblank(a) __raven_isblank(a)
#define iscntrl(a) __raven_iscntrl(a)
#define isdigit(a) __raven_isdigit(a)
#define isgraph(a) __raven_isgraph(a)
#define islower(a) __raven_islower(a)
#define isprint(a) __raven_isprint(a)
#define ispunct(a) __raven_ispunct(a)
#define isspace(a) __raven_isspace(a)
#define isupper(a) __raven_isupper(a)
#define isxdigit(a) __raven_isxdigit(a)
#define tolower(a) __raven_tolower(a)
#define toupper(a) __raven_toupper(a)
#define isascii(a) __raven_isascii(a)
#define toascii(a) __raven_toascii(a)

#define _tolower(a) ((a) | 0x20)
#define _toupper(a) ((a) & 0x5f)
#endif

__END_DECLS

#endif
