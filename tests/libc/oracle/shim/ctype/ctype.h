/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Minimal oracle-only <ctype.h> so the musl donor sources are not rewritten by the host glibc
 * macros (which expand even at -O2). The declarations are the C standard ones.
 */
#ifndef RAVEN_ORACLE_CTYPE_H
#define RAVEN_ORACLE_CTYPE_H

typedef struct __locale_struct *locale_t;

int isalnum(int);
int isalpha(int);
int isascii(int);
int isblank(int);
int iscntrl(int);
int isdigit(int);
int isgraph(int);
int islower(int);
int isprint(int);
int ispunct(int);
int isspace(int);
int isupper(int);
int isxdigit(int);
int toascii(int);
int tolower(int);
int toupper(int);

#endif
