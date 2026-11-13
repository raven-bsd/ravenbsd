/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/ttydefaults.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _SYS_TTYDEFAULTS_H
#define _SYS_TTYDEFAULTS_H
#include <sys/cdefs.h>

#define TTYDEF_IFLAG (BRKINT | ISTRIP | ICRNL | IMAXBEL | IXON | IXANY)
#define TTYDEF_OFLAG (OPOST | ONLCR | XTABS)
#define TTYDEF_LFLAG (ECHO | ICANON | ISIG | IEXTEN | ECHOE|ECHOKE|ECHOCTL)
#define TTYDEF_CFLAG (CREAD | CS7 | PARENB | HUPCL)
#define TTYDEF_SPEED (B9600)
#define CTRL(x) ((x)&037)
#define CEOF CTRL('d')

#define CEOL '\0'
#define CSTATUS '\0'

#define CERASE 0177
#define CINTR CTRL('c')
#define CKILL CTRL('u')
#define CMIN 1
#define CQUIT 034
#define CSUSP CTRL('z')
#define CTIME 0
#define CDSUSP CTRL('y')
#define CSTART CTRL('q')
#define CSTOP CTRL('s')
#define CLNEXT CTRL('v')
#define CDISCARD CTRL('o')
#define CWERASE CTRL('w')
#define CREPRINT CTRL('r')
#define CEOT CEOF
#define CBRK CEOL
#define CRPRNT CREPRINT
#define CFLUSH CDISCARD

#endif
