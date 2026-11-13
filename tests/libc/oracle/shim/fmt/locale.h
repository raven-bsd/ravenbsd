/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <locale.h> for the FreeBSD printf oracle, written from scratch:
 * the host C locale plus a localeconv_l() whose LC_NUMERIC data carries a
 * thousands separator "," and the grouping string "\3\3" so the ' flag cells
 * are exercised.  The gdtoa shim header also defines localeconv_l; the macro
 * below redirects the printf object to this one.
 */
#ifndef RL_FMT_LOCALE_H
#define RL_FMT_LOCALE_H

#include_next <locale.h>

static struct lconv __rl_fmt_lconv = {
	.decimal_point = (char *)".",
	.thousands_sep = (char *)",",
	.grouping = (char *)"\3\3",
};

static inline struct lconv *
__rl_fmt_localeconv_l(locale_t loc)
{
	(void)loc;
	return (&__rl_fmt_lconv);
}

#define localeconv_l(loc) __rl_fmt_localeconv_l(loc)

#endif
