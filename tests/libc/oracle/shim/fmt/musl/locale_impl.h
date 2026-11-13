/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement locale_impl.h for the musl printf/scanf oracle, written from
 * scratch: the oracle has no locale machinery.  The wide entry points only
 * need an lvalue for CURRENT_LOCALE and the C_LOCALE/UTF8_LOCALE constants;
 * the process runs in the C.UTF-8 locale, so MB_CUR_MAX is 4.
 */
#ifndef RL_FMT_MUSL_LOCALE_IMPL_H
#define RL_FMT_MUSL_LOCALE_IMPL_H

#include <locale.h>

static locale_t __rl_fmt_current_locale;
#define CURRENT_LOCALE (__rl_fmt_current_locale)
#define CURRENT_UTF8 0
#define C_LOCALE ((locale_t)0)
#define UTF8_LOCALE ((locale_t)0)
#undef MB_CUR_MAX
#define MB_CUR_MAX 4

#endif
