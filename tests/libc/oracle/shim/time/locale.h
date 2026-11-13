/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD oracle shim: a minimal C-locale <locale.h> for the tzcode
 * differential. When the oracle reaches this header first, `locale_t` is the
 * oracle's own `struct _locale` so the donor's `part_impl[LC_TIME]` access is
 * valid. When the host headers already defined `locale_t`, the host C locale
 * is used unchanged (the conversion code does not touch `part_impl`).
 */
#ifndef _ORACLE_LOCALE_H
#define _ORACLE_LOCALE_H

#if defined(_BITS_TYPES_LOCALE_T_H) || defined(_BITS_TYPES___LOCALE_T_H)
/* The host already supplied locale_t, struct lconv and the LC_* values. */
#else

#define _BITS_TYPES___LOCALE_T_H 1
#define _BITS_TYPES_LOCALE_T_H 1

#define LC_CTYPE 0
#define LC_NUMERIC 1
#define LC_TIME 2
#define LC_COLLATE 3
#define LC_MONETARY 4
#define LC_MESSAGES 5
#define LC_ALL 6

struct _locale;
typedef struct _locale *locale_t;

struct lconv {
    char *decimal_point;
    char *thousands_sep;
    char *grouping;
    char *int_curr_symbol;
    char *currency_symbol;
    char *mon_decimal_point;
    char *mon_thousands_sep;
    char *mon_grouping;
    char *positive_sign;
    char *negative_sign;
    char int_frac_digits;
    char frac_digits;
    char p_cs_precedes;
    char p_sep_by_space;
    char n_cs_precedes;
    char n_sep_by_space;
    char p_sign_posn;
    char n_sign_posn;
    char int_p_cs_precedes;
    char int_n_cs_precedes;
    char int_p_sep_by_space;
    char int_n_sep_by_space;
    char int_p_sign_posn;
    char int_n_sign_posn;
};

char *setlocale(int, const char *);
struct lconv *localeconv(void);

#endif

#endif
