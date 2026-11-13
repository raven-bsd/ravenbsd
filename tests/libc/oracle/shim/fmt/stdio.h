/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <stdio.h> for the FreeBSD printf oracle, written from scratch.
 * It provides the private FILE image the donor vfprintf.c/vfwprintf.c read and
 * the string-stream flags; the public glibc stdio is not used by the oracle.
 */
#ifndef RL_FMT_STDIO_H
#define RL_FMT_STDIO_H

#include <stddef.h>
#include <stdarg.h>
#include <sys/types.h>
#include <bits/types/mbstate_t.h>
#include <bits/types/wint_t.h>

typedef __builtin_va_list __va_list;
typedef off_t fpos_t;

#ifndef __FILE_defined
#define __FILE_defined 1
#endif
struct _IO_FILE;
typedef struct _IO_FILE FILE;

#include <pthread.h>

#define EOF (-1)
#define BUFSIZ 4096
#define NL_TEXTMAX 128
#define NL_ARGMAX 64

#define __SLBF 0x0001
#define __SNBF 0x0002
#define __SRD  0x0004
#define __SWR  0x0008
#define __SRW  0x0010
#define __SEOF 0x0020
#define __SERR 0x0040
#define __SMBF 0x0080
#define __SSTR 0x0200
#define __SOPT 0x0400
#define __SNPT 0x0800
#define __SOFF 0x1000
#define __SMOD 0x2000
#define __SALC 0x4000

struct _IO_FILE {
	unsigned char *_p;
	int _r;
	int _w;
	short _flags;
	short _file;
	struct {
		unsigned char *_base;
		int _size;
	} _bf;
	int _lbfsize;
	void *_cookie;
	int (*_close)(void *);
	int (*_read)(void *, char *, int);
	fpos_t (*_seek)(void *, fpos_t, int);
	int (*_write)(void *, const char *, int);
	struct {
		unsigned char *_base;
		int _size;
	} _ub;
	int _ur;
	unsigned char _ubuf[3];
	unsigned char _nbuf[1];
	struct {
		unsigned char *_base;
		int _size;
	} _lb;
	int _blksize;
	fpos_t _offset;
	int _orientation;
	mbstate_t _mbstate;
	int _flags2;
	pthread_mutex_t _fl_mutex;
};

#define __sferror(p) ((int)(((p)->_flags & __SERR) != 0))

#define FAKE_FILE { 0 }

#define __printflike(fmtarg, firstvararg) \
	__attribute__((__format__(__printf__, fmtarg, firstvararg)))
#define __noinline __attribute__((__noinline__))
#define __unused __attribute__((__unused__))
#define __unused2 __attribute__((__unused__))
#define __unbounded

struct __suio;
struct __siov;

extern int __sfvwrite(FILE *, struct __suio *);
extern int __swsetup(FILE *);
extern int __fflush(FILE *);
extern int __fseeko(FILE *, off_t, int, int);
extern int __sflush(FILE *);
extern int __sflags(const char *, int *);

static int __isthreaded = 0;
static int __use_xprintf = 0;

int snprintf(char *, size_t, const char *, ...);
int vsnprintf(char *, size_t, const char *, __va_list);
int swprintf(wchar_t *, size_t, const wchar_t *, ...);
int vswprintf(wchar_t *, size_t, const wchar_t *, __va_list);

#endif
