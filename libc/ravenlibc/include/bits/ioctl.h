/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:arch/generic/bits/ioctl.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * RavenBSD S742/DR-69: terminal and generic-file requests use the selected FreeBSD
 * 133344405f532bd54ff3629a77d4c4e062f01fdb _IOC encoding (direction in bits 29-30,
 * length in bits 16-28, group in bits 8-15, number in bits 0-7). The numeric interface
 * facts are original expression here; the private tty scheme wire is unaffected.
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
#ifndef _BITS_IOCTL_H
#define _BITS_IOCTL_H

#define IOCPARM_SHIFT 13
#define IOCPARM_MASK  ((1 << IOCPARM_SHIFT) - 1)
#define IOCPARM_LEN(x) (((x) >> 16) & IOCPARM_MASK)
#define IOCBASECMD(x) ((x) & ~(IOCPARM_MASK << 16))
#define IOCGROUP(x)   (((x) >> 8) & 0xff)
#define IOCNUM(x)     ((x) & 0xff)
#define IOCPARM_MAX   (1 << IOCPARM_SHIFT)

#define IOC_VOID   0x20000000UL
#define IOC_OUT    0x40000000UL
#define IOC_IN     0x80000000UL
#define IOC_INOUT  (IOC_IN | IOC_OUT)
#define IOC_DIRMASK (IOC_VOID | IOC_OUT | IOC_IN)

#define _IOC(inout, group, num, len) \
	((unsigned long)((inout) | (((len) & IOCPARM_MASK) << 16) | ((group) << 8) | (num)))
#define _IO(g, n)       _IOC(IOC_VOID, (g), (n), 0)
#define _IOWINT(g, n)   _IOC(IOC_VOID, (g), (n), sizeof(int))
#define _IOR(g, n, t)   _IOC(IOC_OUT, (g), (n), sizeof(t))
#define _IOW(g, n, t)   _IOC(IOC_IN, (g), (n), sizeof(t))
#define _IOWR(g, n, t)  _IOC(IOC_INOUT, (g), (n), sizeof(t))
#define _IOC_NEWLEN(ioc, len) \
	(((~(IOCPARM_MASK << 16)) & (ioc)) | (((len) & IOCPARM_MASK) << 16))
#define _IOC_NEWTYPE(ioc, type) _IOC_NEWLEN((ioc), sizeof(type))

/* Terminal requests (FreeBSD sys/ttycom.h). */
#define TIOCEXCL        _IO('t', 13)
#define TIOCNXCL        _IO('t', 14)
#define TIOCGPTN        _IOR('t', 15, int)
#define TIOCFLUSH       _IOW('t', 16, int)
#define TIOCGETA        _IOR('t', 19, struct termios)
#define TIOCSETA        _IOW('t', 20, struct termios)
#define TIOCSETAW       _IOW('t', 21, struct termios)
#define TIOCSETAF       _IOW('t', 22, struct termios)
#define TIOCGETD        _IOR('t', 26, int)
#define TIOCSETD        _IOW('t', 27, int)
#define TIOCPTMASTER    _IO('t', 28)
#define TIOCGDRAINWAIT  _IOR('t', 86, int)
#define TIOCSDRAINWAIT  _IOW('t', 87, int)
#define TIOCTIMESTAMP   _IOR('t', 89, struct timeval)
#define TIOCDRAIN       _IO('t', 94)
#define TIOCSIG         _IOWINT('t', 95)
#define TIOCEXT         _IOW('t', 96, int)
#define TIOCSCTTY       _IO('t', 97)
#define TIOCCONS        _IOW('t', 98, int)
#define TIOCGSID        _IOR('t', 99, int)
#define TIOCSTAT        _IO('t', 101)
#define TIOCUCNTL       _IOW('t', 102, int)
#define UIOCCMD(n)      _IO('u', n)
#define TIOCSWINSZ      _IOW('t', 103, struct winsize)
#define TIOCGWINSZ      _IOR('t', 104, struct winsize)
#define TIOCMGET        _IOR('t', 106, int)
#define TIOCMBIC        _IOW('t', 107, int)
#define TIOCMBIS        _IOW('t', 108, int)
#define TIOCMSET        _IOW('t', 109, int)
#define TIOCSTART       _IO('t', 110)
#define TIOCSTOP        _IO('t', 111)
#define TIOCPKT         _IOW('t', 112, int)
#define TIOCNOTTY       _IO('t', 113)
#define TIOCSTI         _IOW('t', 114, char)
#define TIOCOUTQ        _IOR('t', 115, int)
#define TIOCSPGRP       _IOW('t', 118, int)
#define TIOCGPGRP       _IOR('t', 119, int)
#define TIOCCDTR        _IO('t', 120)
#define TIOCSDTR        _IO('t', 121)
#define TIOCCBRK        _IO('t', 122)
#define TIOCSBRK        _IO('t', 123)

/* Compatibility aliases for the Linux-shaped names used by older clients. */
#define TCGETS   TIOCGETA
#define TCSETS   TIOCSETA
#define TCSETSW  TIOCSETAW
#define TCSETSF  TIOCSETAF

#define TIOCM_LE   0001
#define TIOCM_DTR  0002
#define TIOCM_RTS  0004
#define TIOCM_ST   0010
#define TIOCM_SR   0020
#define TIOCM_CTS  0040
#define TIOCM_DCD  0100
#define TIOCM_RI   0200
#define TIOCM_DSR  0400
#define TIOCM_CD   TIOCM_DCD
#define TIOCM_CAR  TIOCM_DCD
#define TIOCM_RNG  TIOCM_RI

#define TTYDISC      0
#define SLIPDISC     4
#define PPPDISC      5
#define NETGRAPHDISC 6
#define H4DISC       7

/* Generic descriptor requests (FreeBSD sys/filio.h). */
#define FIOCLEX       _IO('f', 1)
#define FIONCLEX      _IO('f', 2)
#define FIONREAD      _IOR('f', 127, int)
#define FIONBIO       _IOW('f', 126, int)
#define FIOASYNC      _IOW('f', 125, int)
#define FIOSETOWN     _IOW('f', 124, int)
#define FIOGETOWN     _IOR('f', 123, int)
#define FIODTYPE      _IOR('f', 122, int)
#define FIOGETLBA     _IOR('f', 121, int)
#define FIODGNAME     _IOW('f', 120, struct fiodgname_arg)
#define FIONWRITE     _IOR('f', 119, int)
#define FIONSPACE     _IOR('f', 118, int)
#define FIOSEEKDATA   _IOWR('f', 97, off_t)
#define FIOSEEKHOLE   _IOWR('f', 98, off_t)
#define FIOBMAP2      _IOWR('f', 99, struct fiobmap2_arg)
#define FIOSSHMLPGCNF _IOW('f', 100, struct shm_largepage_conf)
#define FIOGSHMLPGCNF _IOR('f', 101, struct shm_largepage_conf)

struct fiodgname_arg {
	int len;
	void *buf;
};

struct fiobmap2_arg {
	long bn;
	int runp;
	int runb;
};

#endif /* _BITS_IOCTL_H */
