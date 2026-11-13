/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/uchar.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _UCHAR_H
#define _UCHAR_H

#include <sys/cdefs.h>

#define __NEED_mbstate_t
#define __NEED_size_t
#include <bits/alltypes.h>

__BEGIN_DECLS

#if !defined(__cplusplus) || __cplusplus < 201103L
typedef unsigned short char16_t;
typedef unsigned int char32_t;
#endif

/*
 * RavenBSD: char8_t comes from the compiler when it has the C23 keyword (or C++20), and is a
 * typedef otherwise. The shell's prototypes still use the type name.
 */
#if defined(__cplusplus)
#  if __cplusplus < 202002L
typedef unsigned char char8_t;
#  endif
#elif !defined(__STDC_VERSION__) || __STDC_VERSION__ < 202311L
typedef unsigned char char8_t;
#endif

#define __STDC_UTF_16__ 1
#define __STDC_UTF_32__ 1
#define __STDC_UTF_8__ 1

size_t c16rtomb(char *__restrict, char16_t, mbstate_t *__restrict);
size_t mbrtoc16(char16_t *__restrict, const char *__restrict, size_t, mbstate_t *__restrict);
size_t c32rtomb(char *__restrict, char32_t, mbstate_t *__restrict);
size_t mbrtoc32(char32_t *__restrict, const char *__restrict, size_t, mbstate_t *__restrict);

size_t c8rtomb(char *__restrict, char8_t, mbstate_t *__restrict);
size_t mbrtoc8(char8_t *__restrict, const char *__restrict, size_t, mbstate_t *__restrict);

__END_DECLS

#endif
