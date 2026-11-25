/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:arch/generic/bits/termios.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * RavenBSD S742/DR-69: the public domain is the selected FreeBSD
 * 133344405f532bd54ff3629a77d4c4e062f01fdb record and constant set. The numeric values are
 * interface facts; this expression is original. The private tty wire record keeps its own
 * 60-byte shape in raven-tty-core/raven-proto-tty and is translated at the libc seam.
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
#ifndef _BITS_TERMIOS_H
#define _BITS_TERMIOS_H

/* `include/termios.h` keeps its base `NCCS 32`; the public record is 20. */
#undef NCCS
#define NCCS 20

#define _POSIX_VDISABLE 0xff

/* Control-character indexes (FreeBSD _termios.h). */
#define VEOF      0
#define VEOL      1
#define VEOL2     2
#define VERASE    3
#define VWERASE   4
#define VKILL     5
#define VREPRINT  6
#define VERASE2   7
#define VINTR     8
#define VQUIT     9
#define VSUSP    10
#define VDSUSP   11
#define VSTART   12
#define VSTOP    13
#define VLNEXT   14
#define VDISCARD 15
#define VMIN     16
#define VTIME    17
#define VSTATUS  18

/* Input flags. */
#define IGNBRK  0x00000001
#define BRKINT  0x00000002
#define IGNPAR  0x00000004
#define PARMRK  0x00000008
#define INPCK   0x00000010
#define ISTRIP  0x00000020
#define INLCR   0x00000040
#define IGNCR   0x00000080
#define ICRNL   0x00000100
#define IXON    0x00000200
#define IXOFF   0x00000400
#define IXANY   0x00000800
#define IMAXBEL 0x00002000
#define IUTF8   0x00004000

/* Output flags. */
#define OPOST    0x00000001
#define ONLCR    0x00000002
#define TABDLY   0x00000004
#define TAB0     0x00000000
#define TAB3     0x00000004
#define ONOEOT   0x00000008
#define OCRNL    0x00000010
#define ONOCR    0x00000020
#define ONLRET   0x00000040

/* Control flags. */
#define CIGNORE     0x00000001
#define CSIZE       0x00000300
#define CS5         0x00000000
#define CS6         0x00000100
#define CS7         0x00000200
#define CS8         0x00000300
#define CSTOPB      0x00000400
#define CREAD       0x00000800
#define PARENB      0x00001000
#define PARODD      0x00002000
#define HUPCL       0x00004000
#define CLOCAL      0x00008000
#define CCTS_OFLOW  0x00010000
#define CRTSCTS     (CCTS_OFLOW | CRTS_IFLOW)
#define CRTS_IFLOW  0x00020000
#define CDTR_IFLOW  0x00040000
#define CDSR_OFLOW  0x00080000
#define CCAR_OFLOW  0x00100000
#define CNO_RTSDTR  0x00200000

/* Local flags. */
#define ECHOKE      0x00000001
#define ECHOE       0x00000002
#define ECHOK       0x00000004
#define ECHO        0x00000008
#define ECHONL      0x00000010
#define ECHOPRT     0x00000020
#define ECHOCTL     0x00000040
#define ISIG        0x00000080
#define ICANON      0x00000100
#define ALTWERASE   0x00000200
#define IEXTEN      0x00000400
#define EXTPROC     0x00000800
#define TOSTOP      0x00400000
#define FLUSHO      0x00800000
#define NOKERNINFO  0x02000000
#define PENDIN      0x20000000
#define NOFLSH      0x80000000

/* Standard speeds. */
#define B0       0
#define B50      50
#define B75      75
#define B110     110
#define B134     134
#define B150     150
#define B200     200
#define B300     300
#define B600     600
#define B1200    1200
#define B1800    1800
#define B2400    2400
#define B4800    4800
#define B9600    9600
#define B19200   19200
#define B38400   38400
#define B7200    7200
#define B14400   14400
#define B28800   28800
#define B57600   57600
#define B76800   76800
#define B115200  115200
#define B230400  230400
#define B460800  460800
#define B500000  500000
#define B921600  921600
#define B1000000 1000000U
#define B1500000 1500000U
#define B2000000 2000000U
#define B2500000 2500000U
#define B3000000 3000000U
#define B3500000 3500000U
#define B4000000 4000000U
#define EXTA     19200
#define EXTB     38400

/* tcsetattr() actions and tcflush()/tcflow() selectors (FreeBSD termios.h). */
#define TCSANOW   0
#define TCSADRAIN 1
#define TCSAFLUSH 2
#define TCSASOFT  0x10
#define TCIFLUSH  1
#define TCOFLUSH  2
#define TCIOFLUSH 3
#define TCOOFF    1
#define TCOON     2
#define TCIOFF    3
#define TCION     4

/* RavenBSD compatibility aliases for the BSD-visible spellings. */
#define OXTABS  TAB3
#define XTABS   TAB3
#define MDMBUF  CCAR_OFLOW

struct termios {
	tcflag_t c_iflag;
	tcflag_t c_oflag;
	tcflag_t c_cflag;
	tcflag_t c_lflag;
	cc_t c_cc[NCCS];
	speed_t c_ispeed;
	speed_t c_ospeed;
};

#endif /* _BITS_TERMIOS_H */
