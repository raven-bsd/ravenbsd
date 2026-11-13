/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/wordexp.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_WORDEXP_H
#define	_WORDEXP_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_size_t

#include <bits/alltypes.h>

#define WRDE_DOOFFS  1
#define WRDE_APPEND  2
#define WRDE_NOCMD   4
#define WRDE_REUSE   8
#define WRDE_SHOWERR 16
#define WRDE_UNDEF   32

typedef struct {
	size_t we_wordc;
	char **we_wordv;
	size_t we_offs;
} wordexp_t;

#define WRDE_NOSYS   -1
#define WRDE_NOSPACE 1
#define WRDE_BADCHAR 2
#define WRDE_BADVAL  3
#define WRDE_CMDSUB  4
#define WRDE_SYNTAX  5

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int wordexp (const char *__restrict, wordexp_t *__restrict, int);
void wordfree (wordexp_t *);

#endif
__END_DECLS

#endif
