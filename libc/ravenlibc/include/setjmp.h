/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/setjmp.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_SETJMP_H
#define	_SETJMP_H

#include <sys/cdefs.h>
#include <bits/setjmp.h>

__BEGIN_DECLS

typedef struct __jmp_buf_tag {
	__jmp_buf __jb;
	unsigned long __fl;
	unsigned long __ss[128/sizeof(long)];
} jmp_buf[1];

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
typedef jmp_buf sigjmp_buf;
int sigsetjmp (sigjmp_buf, int) __returns_twice;
__dead2 void siglongjmp (sigjmp_buf, int);
#endif

#if __XSI_VISIBLE || __BSD_VISIBLE
int _setjmp (jmp_buf) __returns_twice;
__dead2 void _longjmp (jmp_buf, int);
#endif

int setjmp (jmp_buf) __returns_twice;
__dead2 void longjmp (jmp_buf, int);

#define setjmp setjmp

__END_DECLS

#endif
