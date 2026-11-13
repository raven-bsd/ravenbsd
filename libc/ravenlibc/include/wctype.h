/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/wctype.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _WCTYPE_H
#define _WCTYPE_H

#include <sys/cdefs.h>

#define __NEED_wint_t
#define __NEED_wctype_t
#define __NEED_locale_t
#include <bits/alltypes.h>

__BEGIN_DECLS

typedef const int *wctrans_t;

#undef WEOF
/* RavenBSD: WEOF is (wint_t)-1 on every architecture (doc 14 D4). */
#define WEOF ((wint_t)-1)

#undef iswdigit

int iswalnum(wint_t);
int iswalpha(wint_t);
int iswblank(wint_t);
int iswcntrl(wint_t);
int iswdigit(wint_t);
int iswgraph(wint_t);
int iswlower(wint_t);
int iswprint(wint_t);
int iswpunct(wint_t);
int iswspace(wint_t);
int iswupper(wint_t);
int iswxdigit(wint_t);
int iswctype(wint_t, wctype_t);
wint_t towctrans(wint_t, wctrans_t);
wint_t towlower(wint_t);
wint_t towupper(wint_t);
wctrans_t wctrans(const char *);
wctype_t wctype(const char *);

int iswalnum_l(wint_t, locale_t);
int iswalpha_l(wint_t, locale_t);
int iswblank_l(wint_t, locale_t);
int iswcntrl_l(wint_t, locale_t);
int iswdigit_l(wint_t, locale_t);
int iswgraph_l(wint_t, locale_t);
int iswlower_l(wint_t, locale_t);
int iswprint_l(wint_t, locale_t);
int iswpunct_l(wint_t, locale_t);
int iswspace_l(wint_t, locale_t);
int iswupper_l(wint_t, locale_t);
int iswxdigit_l(wint_t, locale_t);
int iswctype_l(wint_t, wctype_t, locale_t);
wint_t towlower_l(wint_t, locale_t);
wint_t towupper_l(wint_t, locale_t);
wint_t towctrans_l(wint_t, wctrans_t, locale_t);
wctrans_t wctrans_l(const char *, locale_t);
wctype_t wctype_l(const char *, locale_t);

#ifndef __cplusplus
static __inline int __raven_iswdigit(wint_t _wc)
{
	return ((unsigned)_wc - '0') < 10;
}

#define iswdigit(a) __raven_iswdigit(a)
#endif

__END_DECLS

#endif
