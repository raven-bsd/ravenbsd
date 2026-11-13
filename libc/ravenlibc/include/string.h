/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/string.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 * plus the BSD extensions of doc 01 3.7 and the XSI/GNU strerror_r split of doc 01 3.6.
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
#ifndef _STRING_H
#define _STRING_H

#include <sys/cdefs.h>

__BEGIN_DECLS

#define __NEED_size_t
#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
#define __NEED_locale_t
#endif
#if __BSD_VISIBLE
#define __NEED_mode_t
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

void *memcpy(void *__restrict, const void *__restrict, size_t);
void *memmove(void *, const void *, size_t);
void *memset(void *, int, size_t);
int memcmp(const void *, const void *, size_t);
void *memchr(const void *, int, size_t);

#if __POSIX_VISIBLE >= 202405 || __XSI_VISIBLE >= 800 || __BSD_VISIBLE || __ISO_C_VISIBLE >= 2023
void *memccpy(void *__restrict, const void *__restrict, int, size_t);
#endif

/* C23: like explicit_bzero but with a fill byte (doc 01 3.7). */
void memset_explicit(void *, int, size_t);

char *strcpy(char *__restrict, const char *__restrict);
char *strncpy(char *__restrict, const char *__restrict, size_t);

char *strcat(char *__restrict, const char *__restrict);
char *strncat(char *__restrict, const char *__restrict, size_t);

int strcmp(const char *, const char *);
int strncmp(const char *, const char *, size_t);

int strcoll(const char *, const char *);
size_t strxfrm(char *__restrict, const char *__restrict, size_t);

char *strchr(const char *, int);
char *strrchr(const char *, int);

size_t strcspn(const char *, const char *);
size_t strspn(const char *, const char *);
char *strpbrk(const char *, const char *);
char *strstr(const char *, const char *);
char *strtok(char *__restrict, const char *__restrict);

size_t strlen(const char *);

char *strerror(int);

#if __POSIX_VISIBLE || __XSI_VISIBLE || __BSD_VISIBLE
char *strtok_r(char *__restrict, const char *__restrict, char **__restrict);
char *stpcpy(char *__restrict, const char *__restrict);
char *stpncpy(char *__restrict, const char *__restrict, size_t);
size_t strnlen(const char *, size_t);
char *strdup(const char *);
char *strndup(const char *, size_t);
char *strsignal(int);
char *strerror_l(int, locale_t);
int strcoll_l(const char *, const char *, locale_t);
size_t strxfrm_l(char *__restrict, const char *__restrict, size_t, locale_t);
void *memmem(const void *, size_t, const void *, size_t);
#endif

/*
 * RavenBSD: the exported symbol is the XSI `int` form (doc 01 3.6). Under _GNU_SOURCE the name
 * redirects to `__gnu_strerror_r`, which returns `char *` and always fills the buffer.
 */
#if __POSIX_VISIBLE >= 200112 || __XSI_VISIBLE || __BSD_VISIBLE
int strerror_r(int, char *, size_t);
#endif
#if __BSD_VISIBLE
int __xpg_strerror_r(int, char *, size_t);
#endif
#if defined(_GNU_SOURCE)
#define strerror_r __gnu_strerror_r
char *__gnu_strerror_r(int, char *, size_t);
#endif

#if __BSD_VISIBLE
size_t strlcpy(char *__restrict, const char *__restrict, size_t);
size_t strlcat(char *__restrict, const char *__restrict, size_t);
char *strsep(char **, const char *);
char *strnstr(const char *, const char *, size_t);
void explicit_bzero(void *, size_t);
int timingsafe_bcmp(const void *, const void *, size_t);
int timingsafe_memcmp(const void *, const void *, size_t);
void strmode(mode_t, char *);
char *__gnu_basename(const char *);
#endif

#if defined(_GNU_SOURCE)
#define basename __gnu_basename
#endif

#if __BSD_VISIBLE
int strverscmp(const char *, const char *);
char *strchrnul(const char *, int);
char *strcasestr(const char *, const char *);
void *memrchr(const void *, int, size_t);
void *mempcpy(void *__restrict, const void *__restrict, size_t);
void *rawmemchr(const void *, int);
#endif

#if __BSD_VISIBLE
#include <strings.h>
#endif

__END_DECLS

#endif /* _STRING_H */
