/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: NetBSD's gdtoa takes a locale_t and calls localeconv_l()/_current_locale();
 * the oracle runs in the C locale, so both collapse to the host C locale. The donor's __UNCONST
 * cast (sys/cdefs.h) is supplied here because the host header does not define it.
 */
#ifndef _ORACLE_GDTOA_SETLOCALE_LOCAL_H
#define _ORACLE_GDTOA_SETLOCALE_LOCAL_H

#include <locale.h>

#ifndef __UNCONST
#define __UNCONST(a) ((char *)(uintptr_t)(a))
#endif

/* gdtoa only reads the decimal point; the oracle runs in the host's global C locale. */
static inline locale_t
_current_locale(void)
{
	return LC_GLOBAL_LOCALE;
}

#endif
