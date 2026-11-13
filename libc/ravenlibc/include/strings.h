/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/strings.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 * plus the BSD `fls` family of doc 01 3.7.
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
#ifndef _STRINGS_H
#define _STRINGS_H

#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_size_t
#define __NEED_locale_t
#include <bits/alltypes.h>

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int bcmp(const void *, const void *, size_t);
void bcopy(const void *, void *, size_t);
void bzero(void *, size_t);
char *index(const char *, int);
char *rindex(const char *, int);
#endif

int ffs(int);
#if __XSI_VISIBLE || __BSD_VISIBLE
int ffsl(long);
int ffsll(long long);
#endif

int strcasecmp(const char *, const char *);
int strncasecmp(const char *, const char *, size_t);

#if __POSIX_VISIBLE >= 200809 || __BSD_VISIBLE
int strcasecmp_l(const char *, const char *, locale_t);
int strncasecmp_l(const char *, const char *, size_t, locale_t);
#endif

#if __BSD_VISIBLE
int fls(int);
int flsl(long);
int flsll(long long);
#endif

__END_DECLS

#endif /* _STRINGS_H */
