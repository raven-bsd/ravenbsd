/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD oracle shim: one immutable C-locale object. The donor calls
 * `_current_locale()` and indexes `part_impl[LC_TIME]`.
 */
#ifndef _ORACLE_SETLOCALE_LOCAL_H
#define _ORACLE_SETLOCALE_LOCAL_H

#include <locale.h>
#include <sys/localedef.h>

struct state;
struct tm;
extern struct state *__lcl_ptr;
size_t strftime_lz(struct state *, char *, size_t, const char *, const struct tm *,
                   locale_t);

struct _locale {
    void *part_impl[13];
};

static const _TimeLocale oracle_time_locale = {
    .abday = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"},
    .day = {"Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"},
    .abmon = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"},
    .mon = {"January", "February", "March", "April", "May", "June", "July", "August",
            "September", "October", "November", "December"},
    .am_pm = {"AM", "PM"},
    .d_t_fmt = "%a %b %e %H:%M:%S %Y",
    .d_fmt = "%m/%d/%y",
    .t_fmt = "%H:%M:%S",
    .t_fmt_ampm = "%I:%M:%S %p",
};

static struct _locale oracle_c_locale = {
    .part_impl = {[LC_TIME] = (void *)&oracle_time_locale},
};

static inline locale_t
_current_locale(void)
{
    return &oracle_c_locale;
}

#endif
