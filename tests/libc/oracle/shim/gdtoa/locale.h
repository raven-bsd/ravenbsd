/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the host C library (glibc) has no localeconv_l(); NetBSD gdtoa calls it.
 * The oracle always runs in the C locale, so the replacement ignores the locale object and returns
 * the process locale's lconv. The real <locale.h> is taken from the next include directory.
 */
#ifndef _ORACLE_GDTOA_LOCALE_H
#define _ORACLE_GDTOA_LOCALE_H

#include_next <locale.h>

static inline struct lconv *
localeconv_l(locale_t loc)
{
	(void)loc;
	return localeconv();
}

#endif
