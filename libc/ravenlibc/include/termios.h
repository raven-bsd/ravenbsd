/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/termios.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_TERMIOS_H
#define	_TERMIOS_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_pid_t
#define __NEED_struct_winsize

#include <bits/alltypes.h>

typedef unsigned char cc_t;
typedef unsigned int speed_t;
typedef unsigned int tcflag_t;

#define NCCS 32

#include <bits/termios.h>

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
speed_t cfgetospeed (const struct termios *);
speed_t cfgetispeed (const struct termios *);
int cfsetospeed (struct termios *, speed_t);
int cfsetispeed (struct termios *, speed_t);

int tcgetattr (int, struct termios *);
int tcsetattr (int, int, const struct termios *);

int tcgetwinsize (int, struct winsize *);
int tcsetwinsize (int, const struct winsize *);

int tcsendbreak (int, int);
int tcdrain (int);
int tcflush (int, int);
int tcflow (int, int);

pid_t tcgetsid (int);

#if __BSD_VISIBLE
void cfmakeraw(struct termios *);
int cfsetspeed(struct termios *, speed_t);
#endif

#endif
__END_DECLS

#endif
