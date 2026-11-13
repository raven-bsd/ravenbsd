/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/personality.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _PERSONALITY_H
#define _PERSONALITY_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define UNAME26            0x0020000
#define ADDR_NO_RANDOMIZE  0x0040000
#define FDPIC_FUNCPTRS     0x0080000
#define MMAP_PAGE_ZERO     0x0100000
#define ADDR_COMPAT_LAYOUT 0x0200000
#define READ_IMPLIES_EXEC  0x0400000
#define ADDR_LIMIT_32BIT   0x0800000
#define SHORT_INODE        0x1000000
#define WHOLE_SECONDS      0x2000000
#define STICKY_TIMEOUTS    0x4000000
#define ADDR_LIMIT_3GB     0x8000000

#define PER_LINUX 0
#define PER_LINUX_32BIT ADDR_LIMIT_32BIT
#define PER_LINUX_FDPIC FDPIC_FUNCPTRS
#define PER_SVR4 (1 | STICKY_TIMEOUTS | MMAP_PAGE_ZERO)
#define PER_SVR3 (2 | STICKY_TIMEOUTS | SHORT_INODE)
#define PER_SCOSVR3 (3 | STICKY_TIMEOUTS | WHOLE_SECONDS | SHORT_INODE)
#define PER_OSR5 (3 | STICKY_TIMEOUTS | WHOLE_SECONDS)
#define PER_WYSEV386 (4 | STICKY_TIMEOUTS | SHORT_INODE)
#define PER_ISCR4 (5 | STICKY_TIMEOUTS)
#define PER_BSD 6
#define PER_SUNOS (6 | STICKY_TIMEOUTS)
#define PER_XENIX (7 | STICKY_TIMEOUTS | SHORT_INODE)
#define PER_LINUX32 8
#define PER_LINUX32_3GB (8 | ADDR_LIMIT_3GB)
#define PER_IRIX32 (9 | STICKY_TIMEOUTS)
#define PER_IRIXN32 (0xa | STICKY_TIMEOUTS)
#define PER_IRIX64 (0x0b | STICKY_TIMEOUTS)
#define PER_RISCOS 0xc
#define PER_SOLARIS (0xd | STICKY_TIMEOUTS)
#define PER_UW7 (0xe | STICKY_TIMEOUTS | MMAP_PAGE_ZERO)
#define PER_OSF4 0xf
#define PER_HPUX 0x10
#define PER_MASK 0xff

#if __BSD_VISIBLE
int personality(unsigned long);
#endif

__END_DECLS
#endif
