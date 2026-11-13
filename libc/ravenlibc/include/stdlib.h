/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/stdlib.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 * plus the OpenBSD/FreeBSD BSD extensions of doc 03 sections 3.1-3.4 and 3.8 and the C23 binary
 * prefix documentation. The musl register-alias block and `strtoi/strtou` are not provided
 * (RD-22); `MB_CUR_MAX` is `__ctype_get_mb_cur_max()` (H-3).
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
#ifndef _STDLIB_H
#define _STDLIB_H

#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_size_t
#define __NEED_wchar_t
#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
#define __NEED_locale_t
#endif

#include <bits/alltypes.h>

#ifndef NULL
#if __cplusplus >= 201103L
#define NULL nullptr
#elif defined(__cplusplus)
#define NULL 0L
#else
#define NULL ((void*)0)
#endif
#endif

#define EXIT_FAILURE 1
#define EXIT_SUCCESS 0
#define RAND_MAX 0x7fffffff

typedef struct { int quot, rem; } div_t;
typedef struct { long quot, rem; } ldiv_t;
typedef struct { long long quot, rem; } lldiv_t;

/*
 * RavenBSD: bytes pass through in the C/POSIX locale and decode as UTF-8 otherwise; the current
 * thread's locale decides (H-3). The implementation lives in the locale crate.
 */
#define MB_CUR_MAX (__ctype_get_mb_cur_max())
size_t __ctype_get_mb_cur_max(void);

int atoi (const char *);
long atol (const char *);
long long atoll (const char *);
double atof (const char *);

float strtof (const char *__restrict, char **__restrict);
double strtod (const char *__restrict, char **__restrict);
long double strtold (const char *__restrict, char **__restrict);

long strtol (const char *__restrict, char **__restrict, int);
unsigned long strtoul (const char *__restrict, char **__restrict, int);
long long strtoll (const char *__restrict, char **__restrict, int);
unsigned long long strtoull (const char *__restrict, char **__restrict, int);

int rand (void);
void srand (unsigned);

void *malloc (size_t);
void *calloc (size_t, size_t);
void *realloc (void *, size_t);
void free (void *);
void *aligned_alloc (size_t, size_t);

_Noreturn void abort (void);
int atexit (void (*) (void));
_Noreturn void exit (int);
_Noreturn void _Exit (int);
int at_quick_exit (void (*) (void));
_Noreturn void quick_exit (int);

char *getenv (const char *);

int system (const char *);

void *bsearch (const void *, const void *, size_t, size_t,
	int (*)(const void *, const void *));
void qsort (void *, size_t, size_t, int (*)(const void *, const void *));

int abs (int);
long labs (long);
long long llabs (long long);

div_t div (int, int);
ldiv_t ldiv (long, long);
lldiv_t lldiv (long long, long long);

int mblen (const char *, size_t);
int mbtowc (wchar_t *__restrict, const char *__restrict, size_t);
int wctomb (char *, wchar_t);
size_t mbstowcs (wchar_t *__restrict, const char *__restrict, size_t);
size_t wcstombs (char *__restrict, const wchar_t *__restrict, size_t);

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
int posix_memalign (void **, size_t, size_t);
int rand_r (unsigned *);
char *realpath (const char *__restrict, char *__restrict);
int setenv (const char *, const char *, int);
int unsetenv (const char *);
int putenv (char *);
int mkstemp (char *);
char *mkdtemp (char *);
int getsubopt (char **, char *const *, char **);
#endif

#if __XSI_VISIBLE || __BSD_VISIBLE
extern char *suboptarg;
void srandom (unsigned);
char *initstate (unsigned, char *, size_t);
char *setstate (char *);
long random (void);
long a64l (const char *);
char *l64a (long);
double drand48 (void);
double erand48 (unsigned short [3]);
long lrand48 (void);
long nrand48 (unsigned short [3]);
long mrand48 (void);
long jrand48 (unsigned short [3]);
void srand48 (long);
unsigned short *seed48 (unsigned short [3]);
void lcong48 (unsigned short [7]);
#endif

#if __POSIX_VISIBLE >= 202405 || __BSD_VISIBLE || defined(_GNU_SOURCE)
/* POSIX-2024 / GNU argument order: cmp(a, b, arg). CR-8. */
void qsort_r (void *, size_t, size_t,
	int (*)(const void *, const void *, void *), void *);
#endif

#if __BSD_VISIBLE
int heapsort (void *, size_t, size_t,
	int (*)(const void *, const void *));
int mergesort (void *, size_t, size_t,
	int (*)(const void *, const void *));
long long strtonum (const char *, long long, long long, const char **);
int getenv_r (const char *, char *, size_t);
int clearenv (void);
char *secure_getenv (const char *);
#endif

#if __BSD_VISIBLE || defined(_GNU_SOURCE)
char *ecvt (double, int, int *, int *);
char *fcvt (double, int, int *, int *);
char *gcvt (double, int, char *);
int ecvt_r (double, int, int *, int *, char *, size_t);
int fcvt_r (double, int, int *, int *, char *, size_t);
#endif

/* Itanium C++ ABI (Z02); the function pointers are opaque words in the shell. */
int __cxa_atexit (void (*) (void *), void *, void *);
void __cxa_finalize (void *);
int __cxa_thread_atexit_impl (void (*) (void *), void *, void *);

#if __XSI_VISIBLE
int grantpt (int);
int unlockpt (int);
char *ptsname (int);
int ptsname_r (int, char *, size_t);
int posix_openpt (int);
#endif

__END_DECLS

#endif /* _STDLIB_H */
