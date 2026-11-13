/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <stdio.h> for the musl printf/scanf oracle, written from scratch.
 * It only forward-declares musl's private FILE; src/internal/stdio_impl.h
 * defines struct _IO_FILE.  The host <stdio.h> is deliberately not included so
 * the donor's FILE image is the only one in the translation unit.
 */
#ifndef RL_FMT_MUSL_STDIO_H
#define RL_FMT_MUSL_STDIO_H

#include <stddef.h>
#include <stdarg.h>
#include <sys/types.h>
#include <bits/types/mbstate_t.h>
#include <bits/types/wint_t.h>

#ifndef __FILE_defined
#define __FILE_defined 1
#endif
struct _IO_FILE;
typedef struct _IO_FILE FILE;

#define EOF (-1)
#define BUFSIZ 1024
#define NL_ARGMAX 9

#define hidden /* default visibility: the oracle exports every definition */
#define weak __attribute__((__weak__))
#define weak_alias(old, new) /* the oracle exports the donor name only */

int snprintf(char *, size_t, const char *, ...);
int vsnprintf(char *, size_t, const char *, va_list);
int swprintf(wchar_t *, size_t, const wchar_t *, ...);
int vswprintf(wchar_t *, size_t, const wchar_t *, va_list);
int sscanf(const char *, const char *, ...);
int vsscanf(const char *, const char *, va_list);

#endif
