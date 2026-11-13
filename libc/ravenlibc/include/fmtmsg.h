/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/fmtmsg.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _FMTMSG_H
#define _FMTMSG_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define MM_HARD		1
#define MM_SOFT		2
#define MM_FIRM		4

#define MM_APPL		8
#define MM_UTIL		16
#define MM_OPSYS	32

#define MM_RECOVER	64
#define MM_NRECOV	128

#define MM_PRINT	256
#define MM_CONSOLE	512

#define MM_NULLMC	0L

#define MM_HALT		1
#define MM_ERROR	2
#define MM_WARNING	3
#define MM_INFO		4
#define MM_NOSEV	0

#define MM_OK		0
#define MM_NOTOK	(-1)
#define MM_NOMSG	1
#define MM_NOCON	4

#define MM_NULLLBL	((char*)0)
#define MM_NULLTXT	((char*)0)
#define MM_NULLACT	((char*)0)
#define MM_NULLTAG	((char*)0)
#define MM_NULLSEV	0

int fmtmsg(long, const char *, int, const char *, const char *, const char *);

__END_DECLS

#endif
