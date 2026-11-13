/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/sys/stat.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_SYS_STAT_H
#define	_SYS_STAT_H
#include <sys/cdefs.h>
#include <bits/abi_stat.h>
__BEGIN_DECLS

#define __NEED_dev_t
#define __NEED_ino_t
#define __NEED_mode_t
#define __NEED_nlink_t
#define __NEED_uid_t
#define __NEED_gid_t
#define __NEED_off_t
#define __NEED_time_t
#define __NEED_blksize_t
#define __NEED_blkcnt_t
#define __NEED_struct_timespec

#if __BSD_VISIBLE
#define __NEED_int64_t
#define __NEED_uint64_t
#define __NEED_uint32_t
#define __NEED_uint16_t
#endif

#include <bits/alltypes.h>

#include <bits/stat.h>

#define st_atime st_atim.tv_sec
#define st_mtime st_mtim.tv_sec
#define st_ctime st_ctim.tv_sec

#define S_TYPEISMQ(buf)  0
#define S_TYPEISSEM(buf) 0
#define S_TYPEISSHM(buf) 0
#define S_TYPEISTMO(buf) 0

#define S_ISDIR(mode)  (((mode) & S_IFMT) == S_IFDIR)
#define S_ISCHR(mode)  (((mode) & S_IFMT) == S_IFCHR)
#define S_ISBLK(mode)  (((mode) & S_IFMT) == S_IFBLK)
#define S_ISREG(mode)  (((mode) & S_IFMT) == S_IFREG)
#define S_ISFIFO(mode) (((mode) & S_IFMT) == S_IFIFO)
#define S_ISLNK(mode)  (((mode) & S_IFMT) == S_IFLNK)
#define S_ISSOCK(mode) (((mode) & S_IFMT) == S_IFSOCK)

#ifndef S_IRUSR
#endif

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int stat(const char *__restrict, struct stat *__restrict);
int fstat(int, struct stat *);
int lstat(const char *__restrict, struct stat *__restrict);
int fstatat(int, const char *__restrict, struct stat *__restrict, int);
int chmod(const char *, mode_t);
int fchmod(int, mode_t);
int fchmodat(int, const char *, mode_t, int);
mode_t umask(mode_t);
int mkdir(const char *, mode_t);
int mkfifo(const char *, mode_t);
int mkdirat(int, const char *, mode_t);
int mkfifoat(int, const char *, mode_t);

#if __XSI_VISIBLE || __BSD_VISIBLE
int mknod(const char *, mode_t, dev_t);
int mknodat(int, const char *, mode_t, dev_t);
#endif

int futimens(int, const struct timespec [2]);
int utimensat(int, const char *, const struct timespec [2], int);

#if __BSD_VISIBLE
int lchmod(const char *, mode_t);
#endif

#if __BSD_VISIBLE
#define STATX_TYPE 1U
#define STATX_MODE 2U
#define STATX_NLINK 4U
#define STATX_UID 8U
#define STATX_GID 0x10U
#define STATX_ATIME 0x20U
#define STATX_MTIME 0x40U
#define STATX_CTIME 0x80U
#define STATX_INO 0x100U
#define STATX_SIZE 0x200U
#define STATX_BLOCKS 0x400U
#define STATX_BASIC_STATS 0x7ffU
#define STATX_BTIME 0x800U
#define STATX_ALL 0xfffU
#define STATX_MNT_ID 0x1000U
#define STATX_DIOALIGN 0x2000U
#define STATX_MNT_ID_UNIQUE 0x4000U
#define STATX_SUBVOL 0x8000U
#define STATX_WRITE_ATOMIC 0x10000U

#define STATX_ATTR_COMPRESSED 0x4
#define STATX_ATTR_IMMUTABLE 0x10
#define STATX_ATTR_APPEND 0x20
#define STATX_ATTR_NODUMP 0x40
#define STATX_ATTR_ENCRYPTED 0x800
#define STATX_ATTR_AUTOMOUNT 0x1000
#define STATX_ATTR_MOUNT_ROOT 0x2000
#define STATX_ATTR_VERITY 0x100000
#define STATX_ATTR_DAX 0x200000
#define STATX_ATTR_WRITE_ATOMIC 0x400000

struct statx_timestamp {
	int64_t tv_sec;
	uint32_t tv_nsec, __pad;
};

struct statx {
	uint32_t stx_mask;
	uint32_t stx_blksize;
	uint64_t stx_attributes;
	uint32_t stx_nlink;
	uint32_t stx_uid;
	uint32_t stx_gid;
	uint16_t stx_mode;
	uint16_t __pad0[1];
	uint64_t stx_ino;
	uint64_t stx_size;
	uint64_t stx_blocks;
	uint64_t stx_attributes_mask;
	struct statx_timestamp stx_atime;
	struct statx_timestamp stx_btime;
	struct statx_timestamp stx_ctime;
	struct statx_timestamp stx_mtime;
	uint32_t stx_rdev_major;
	uint32_t stx_rdev_minor;
	uint32_t stx_dev_major;
	uint32_t stx_dev_minor;
	uint64_t stx_mnt_id;
	uint32_t stx_dio_mem_align;
	uint32_t stx_dio_offset_align;
	uint64_t stx_subvol;
	uint32_t stx_atomic_write_unit_min;
	uint32_t stx_atomic_write_unit_max;
	uint32_t stx_atomic_write_segments_max;
	uint32_t __pad1[1];
	uint64_t __pad2[9];

};

int statx(int, const char *__restrict, int, unsigned, struct statx *__restrict);
#endif

#endif
__END_DECLS
#endif
