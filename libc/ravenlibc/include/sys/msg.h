/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/msg.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _SYS_MSG_H
#define _SYS_MSG_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#include <sys/ipc.h>

#define __NEED_pid_t
#define __NEED_key_t
#define __NEED_time_t
#define __NEED_size_t
#define __NEED_ssize_t

#include <bits/alltypes.h>

typedef unsigned long msgqnum_t;
typedef unsigned long msglen_t;

#pragma push_macro("__unused")
#undef __unused
#include <bits/msg.h>
#pragma pop_macro("__unused")

#define __msg_cbytes msg_cbytes

#define MSG_NOERROR 010000
#define MSG_EXCEPT  020000

#define MSG_STAT (11 | (IPC_STAT & 0x100))
#define MSG_INFO 12
#define MSG_STAT_ANY (13 | (IPC_STAT & 0x100))

struct msginfo {
	int msgpool, msgmap, msgmax, msgmnb, msgmni, msgssz, msgtql;
	unsigned short msgseg;
};

int msgctl (int, int, struct msqid_ds *);
int msgget (key_t, int);
ssize_t msgrcv (int, void *, size_t, long, int);
int msgsnd (int, const void *, size_t, int);

#if __BSD_VISIBLE
struct msgbuf {
	long mtype;
	char mtext[1];
};
#endif

__END_DECLS

#endif
