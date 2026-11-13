/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/fcntl.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_FCNTL_H
#define	_FCNTL_H
#include <sys/cdefs.h>
#include <bits/abi_fcntl.h>

__BEGIN_DECLS

#define __NEED_off_t
#define __NEED_pid_t
#define __NEED_mode_t

#if __BSD_VISIBLE
#define __NEED_size_t
#define __NEED_ssize_t
#define __NEED_struct_iovec
#endif

#include <bits/alltypes.h>

#include <bits/fcntl.h>
#undef O_RSYNC
#define O_RSYNC O_SYNC

struct flock {
	short l_type;
	short l_whence;
	off_t l_start;
	off_t l_len;
	pid_t l_pid;
};

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int creat(const char *, mode_t);
int fcntl(int, int, ...);
int open(const char *, int, ...);
int openat(int, const char *, int, ...);
int posix_fadvise(int, off_t, off_t, int);
int posix_fallocate(int, off_t, off_t);

#ifndef POSIX_FADV_DONTNEED
#endif

#undef SEEK_SET
#undef SEEK_CUR
#undef SEEK_END

#ifndef S_IRUSR
#define S_ISUID 04000
#define S_ISGID 02000
#define S_ISVTX 01000
#define S_IRUSR 0400
#define S_IWUSR 0200
#define S_IXUSR 0100
#define S_IRWXU 0700
#define S_IRGRP 0040
#define S_IWGRP 0020
#define S_IXGRP 0010
#define S_IRWXG 0070
#define S_IROTH 0004
#define S_IWOTH 0002
#define S_IXOTH 0001
#define S_IRWXO 0007
#endif

#if __BSD_VISIBLE

#define RWF_WRITE_LIFE_NOT_SET	0
#define RWH_WRITE_LIFE_NONE	1
#define RWH_WRITE_LIFE_SHORT	2
#define RWH_WRITE_LIFE_MEDIUM	3
#define RWH_WRITE_LIFE_LONG	4
#define RWH_WRITE_LIFE_EXTREME	5

#define DN_ACCESS	0x00000001
#define DN_MODIFY	0x00000002
#define DN_CREATE	0x00000004
#define DN_DELETE	0x00000008
#define DN_RENAME	0x00000010
#define DN_ATTRIB	0x00000020
#define DN_MULTISHOT	0x80000000

int lockf(int, int, off_t);
#endif

#if __BSD_VISIBLE
struct file_handle {
	unsigned handle_bytes;
	int handle_type;
	unsigned char f_handle[];
};
struct f_owner_ex {
	int type;
	pid_t pid;
};
#define FALLOC_FL_KEEP_SIZE 1
#define FALLOC_FL_PUNCH_HOLE 2
#define FALLOC_FL_COLLAPSE_RANGE 8
#define FALLOC_FL_ZERO_RANGE 16
#define FALLOC_FL_INSERT_RANGE 32
#define FALLOC_FL_UNSHARE_RANGE 64
#define FALLOC_FL_WRITE_ZEROES 128
#define MAX_HANDLE_SZ 128
#define SYNC_FILE_RANGE_WAIT_BEFORE 1
#define SYNC_FILE_RANGE_WRITE 2
#define SYNC_FILE_RANGE_WAIT_AFTER 4
#define SPLICE_F_MOVE 1
#define SPLICE_F_NONBLOCK 2
#define SPLICE_F_MORE 4
#define SPLICE_F_GIFT 8
int fallocate(int, int, off_t, off_t);
int name_to_handle_at(int, const char *, struct file_handle *, int *, int);
int open_by_handle_at(int, struct file_handle *, int);
ssize_t readahead(int, off_t, size_t);
int sync_file_range(int, off_t, off_t, unsigned);
ssize_t vmsplice(int, const struct iovec *, size_t, unsigned);
ssize_t splice(int, off_t *, int, off_t *, size_t, unsigned);
ssize_t tee(int, int, size_t, unsigned);
#define loff_t off_t
#endif

#endif
__END_DECLS

#endif
