/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/acct.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _SYS_ACCT_H
#define _SYS_ACCT_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#include <time.h>
#include <stdint.h>

#define ACCT_COMM 16

typedef uint16_t comp_t;

struct acct {
	char ac_flag;
	uint16_t ac_uid;
	uint16_t ac_gid;
	uint16_t ac_tty;
	uint32_t ac_btime;
	comp_t ac_utime;
	comp_t ac_stime;
	comp_t ac_etime;
	comp_t ac_mem;
	comp_t ac_io;
	comp_t ac_rw;
	comp_t ac_minflt;
	comp_t ac_majflt;
	comp_t ac_swaps;
	uint32_t ac_exitcode;
	char ac_comm[ACCT_COMM+1];
	char ac_pad[10];
};

struct acct_v3 {
	char ac_flag;
	char ac_version;
	uint16_t ac_tty;
	uint32_t ac_exitcode;
	uint32_t ac_uid;
	uint32_t ac_gid;
	uint32_t ac_pid;
	uint32_t ac_ppid;
	uint32_t ac_btime;
	float ac_etime;
	comp_t ac_utime;
	comp_t ac_stime;
	comp_t ac_mem;
	comp_t ac_io;
	comp_t ac_rw;
	comp_t ac_minflt;
	comp_t ac_majflt;
	comp_t ac_swaps;
	char ac_comm[ACCT_COMM];
};

#define AFORK 1
#define ASU 2
#define ACORE 8
#define AXSIG 16
#define ACCT_BYTEORDER (128*(__BYTE_ORDER==__BIG_ENDIAN))
#define AHZ 100

int acct(const char *);

__END_DECLS

#endif
