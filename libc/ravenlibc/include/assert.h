/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/assert.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6).
 * The text is the H-13 glibc format; `__assert2` and `__assert` are the BSD entry points
 * (B21) and share the builder. C23's variadic assert and the static_assert keyword mapping stay
 * here; `NDEBUG` disables the check.
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
#ifndef _ASSERT_H
#define _ASSERT_H

#include <sys/cdefs.h>

__BEGIN_DECLS

_Noreturn void __assert_fail (const char *, const char *, int, const char *);
_Noreturn void __assert2 (const char *, int, const char *, const char *);
_Noreturn void __assert (const char *, int, const char *);

__END_DECLS

#undef assert

#ifdef NDEBUG
#define assert(...) (void)0
#else
#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 202311L
#define assert(...) ((void)((__VA_ARGS__) || (__assert_fail(#__VA_ARGS__, __FILE__, __LINE__, __func__),0)))
#else
#define assert(x) ((void)((x) || (__assert_fail(#x, __FILE__, __LINE__, __func__),0)))
#endif
#endif

#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L && !defined(__cplusplus)
#define static_assert _Static_assert
#endif

#endif /* _ASSERT_H */
