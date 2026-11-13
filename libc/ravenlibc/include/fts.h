/*
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 1989, 1993
 *	The Regents of the University of California.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 *
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from freebsd:include/fts.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
 */

#ifndef _FTS_H_
#define _FTS_H_

#include <sys/cdefs.h>
#include <sys/types.h>
#include <sys/stat.h>

typedef struct _ftsent FTSENT;

typedef struct {
	FTSENT *fts_cur;
	FTSENT *fts_child;
	FTSENT **fts_array;
	dev_t fts_dev;
	char *fts_path;
	int fts_rfd;
	size_t fts_pathlen;
	size_t fts_nitems;
	union {
		int (*fts_compar)(const FTSENT *const *, const FTSENT *const *);
		void *fts_compar_b;
	};

#define FTS_COMFOLLOW 0x000001
#define FTS_LOGICAL 0x000002
#define FTS_NOCHDIR 0x000004
#define FTS_NOSTAT 0x000008
#define FTS_PHYSICAL 0x000010
#define FTS_SEEDOT 0x000020
#define FTS_XDEV 0x000040
#define FTS_WHITEOUT 0x000080
#define FTS_COMFOLLOWDIR 0x00400
#define FTS_NOSTAT_TYPE 0x000800
#define FTS_OPTIONMASK 0x000cff
#define FTS_NAMEONLY 0x000100
#define FTS_STOP 0x010000
#define FTS_COMPAR_B 0x020000
	int fts_options;
	void *fts_clientptr;
} FTS;

struct _ftsent {
	struct _ftsent *fts_cycle;
	struct _ftsent *fts_parent;
	struct _ftsent *fts_link;
	long long fts_number;
#define fts_bignum fts_number
	void *fts_pointer;
	char *fts_accpath;
	char *fts_path;
	int fts_errno;
	int fts_symfd;
	int fts_dirfd;
	int __fts_reserved[3];
	size_t fts_pathlen;
	size_t fts_namelen;

	ino_t fts_ino;
	dev_t fts_dev;
	nlink_t fts_nlink;

#define FTS_ROOTPARENTLEVEL -1
#define FTS_ROOTLEVEL 0
	long fts_level;

#define FTS_D 1
#define FTS_DC 2
#define FTS_DEFAULT 3
#define FTS_DNR 4
#define FTS_DOT 5
#define FTS_DP 6
#define FTS_ERR 7
#define FTS_F 8
#define FTS_INIT 9
#define FTS_NS 10
#define FTS_NSOK 11
#define FTS_SL 12
#define FTS_SLNONE 13
#define FTS_W 14
	int fts_info;

#define FTS_DONTCHDIR 0x01
#define FTS_SYMFOLLOW 0x02
#define FTS_ISW 0x04
	unsigned fts_flags;

#define FTS_AGAIN 1
#define FTS_FOLLOW 2
#define FTS_NOINSTR 3
#define FTS_SKIP 4
	int fts_instr;

	struct stat *fts_statp;
	char *fts_name;
	FTS *fts_fts;
};

__BEGIN_DECLS
FTSENT *fts_children(FTS *, int);
int fts_close(FTS *);
void *fts_get_clientptr(FTS *);
#define fts_get_clientptr(fts) ((fts)->fts_clientptr)
FTS *fts_get_stream(FTSENT *);
#define fts_get_stream(ftsent) ((ftsent)->fts_fts)
FTS *fts_open(char *const *, int,
    int (*)(const FTSENT *const *, const FTSENT *const *));
FTS *fts_openat(int, char *const *, int,
    int (*)(const FTSENT *const *, const FTSENT *const *));
FTSENT *fts_read(FTS *);
int fts_set(FTS *, FTSENT *, int);
void fts_set_clientptr(FTS *, void *);
__END_DECLS

#endif /* _FTS_H_ */
