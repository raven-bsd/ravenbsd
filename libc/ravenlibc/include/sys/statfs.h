/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/statfs.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_SYS_STATFS_H
#define	_SYS_STATFS_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_fsblkcnt_t
#define __NEED_fsfilcnt_t
#define __NEED_uint32_t
#define __NEED_uint64_t
#define __NEED_int64_t
#define __NEED_uid_t
#include <bits/alltypes.h>

typedef struct __fsid_t {
	int __val[2];
} fsid_t;

#define MFSNAMELEN 16
#define MNAMELEN 1024

/* RavenBSD: public `struct statfs` is the FreeBSD 133344405 layout (DR-69, S733),
 * 2344 bytes. The donor defines it in sys/mount.h; Raven exposes the same record from
 * <sys/statfs.h>. Only numeric layout facts are used. */
struct statfs {
	uint32_t f_version;
	uint32_t f_type;
	uint64_t f_flags;
	uint64_t f_bsize;
	uint64_t f_iosize;
	uint64_t f_blocks;
	uint64_t f_bfree;
	int64_t f_bavail;
	uint64_t f_files;
	int64_t f_ffree;
	uint64_t f_syncwrites;
	uint64_t f_asyncwrites;
	uint64_t f_syncreads;
	uint64_t f_asyncreads;
	uint32_t f_nvnodelistsize;
	uint32_t f_spare0;
	uint64_t f_spare[9];
	uint32_t f_namemax;
	uid_t f_owner;
	fsid_t f_fsid;
	char f_charspare[80];
	char f_fstypename[MFSNAMELEN];
	char f_mntfromname[MNAMELEN];
	char f_mntonname[MNAMELEN];
};

int statfs (const char *, struct statfs *);
int fstatfs (int, struct statfs *);

__END_DECLS

#endif
