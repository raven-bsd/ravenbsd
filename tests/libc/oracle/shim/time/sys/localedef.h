/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD oracle shim: the `_TimeLocale` field interface used by the
 * tzcode strftime/strptime differential. Field names are the donor API.
 */
#ifndef _ORACLE_SYS_LOCALEDEF_H
#define _ORACLE_SYS_LOCALEDEF_H

#include <locale.h>

typedef struct {
    const char *abday[7];
    const char *day[7];
    const char *abmon[12];
    const char *mon[12];
    const char *am_pm[2];
    const char *d_t_fmt;
    const char *d_fmt;
    const char *t_fmt;
    const char *t_fmt_ampm;
} _TimeLocale;

#endif
