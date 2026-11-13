/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the TZif constants and header layout from tzfile.h.
 * These are public-domain facts from RFC 9636 and the tzcode header.
 */
#ifndef _ORACLE_TZFILE_H
#define _ORACLE_TZFILE_H

#define TZDIR "/usr/share/zoneinfo"
#define TZDEFAULT "/etc/localtime"
#define TZDEFRULES "posixrules"

#define TZ_MAGIC "TZif"
struct tzhead {
    char tzh_magic[4];
    char tzh_version[1];
    char tzh_reserved[15];
    char tzh_ttisutcnt[4];
    char tzh_ttisstdcnt[4];
    char tzh_leapcnt[4];
    char tzh_timecnt[4];
    char tzh_typecnt[4];
    char tzh_charcnt[4];
};

#define TZ_MAX_TIMES 2000
#define TZ_MAX_TYPES 256
#define TZ_MAX_CHARS 256
#define TZ_MAX_LEAPS 50

#define SECSPERMIN 60
#define MINSPERHOUR 60
#define HOURSPERDAY 24
#define DAYSPERWEEK 7
#define DAYSPERNYEAR 365
#define DAYSPERLYEAR 366
#define SECSPERHOUR (SECSPERMIN * MINSPERHOUR)
#define SECSPERDAY ((int_fast32_t) SECSPERHOUR * HOURSPERDAY)
#define MONSPERYEAR 12
#define YEARSPERREPEAT 400

#define TM_SUNDAY 0
#define TM_MONDAY 1
#define TM_TUESDAY 2
#define TM_WEDNESDAY 3
#define TM_THURSDAY 4
#define TM_FRIDAY 5
#define TM_SATURDAY 6
#define TM_JANUARY 0
#define TM_FEBRUARY 1
#define TM_MARCH 2
#define TM_APRIL 3
#define TM_MAY 4
#define TM_JUNE 5
#define TM_JULY 6
#define TM_AUGUST 7
#define TM_SEPTEMBER 8
#define TM_OCTOBER 9
#define TM_NOVEMBER 10
#define TM_DECEMBER 11
#define TM_YEAR_BASE 1900
#define TM_WDAY_BASE TM_MONDAY
#define EPOCH_YEAR 1970
#define EPOCH_WDAY TM_THURSDAY

#define isleap(y) (((y) % 4) == 0 && (((y) % 100) != 0 || ((y) % 400) == 0))
#define isleap_sum(a, b) isleap((a) % 400 + (b) % 400)

#endif
