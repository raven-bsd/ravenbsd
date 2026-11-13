/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Minimal oracle-only <wctype.h>: no host macros, just the types and the functions the donor
 * sources call or define.
 */
#ifndef RAVEN_ORACLE_WCTYPE_H
#define RAVEN_ORACLE_WCTYPE_H

typedef struct __locale_struct *locale_t;
typedef unsigned int wint_t;
typedef unsigned long wctype_t;
typedef const int *wctrans_t;

int iswalnum(wint_t);
int iswalpha(wint_t);
int iswblank(wint_t);
int iswcntrl(wint_t);
int iswdigit(wint_t);
int iswgraph(wint_t);
int iswlower(wint_t);
int iswprint(wint_t);
int iswpunct(wint_t);
int iswspace(wint_t);
int iswupper(wint_t);
int iswxdigit(wint_t);
int iswctype(wint_t, wctype_t);
wint_t towctrans(wint_t, wctrans_t);
wint_t towlower(wint_t);
wint_t towupper(wint_t);
wctrans_t wctrans(const char *);
wctype_t wctype(const char *);

#endif
