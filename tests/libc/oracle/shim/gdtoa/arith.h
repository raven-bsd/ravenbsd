/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the donor's arith.h only selects the little-endian IEEE layout on this
 * host (CLEANROOM constant from the amd64 psABI). It is also the first header gdtoa.h includes,
 * so it brings in the host locale_t and tells gdtoa.h not to typedef its own; that keeps the
 * NetBSD locale_t parameter and the host <locale.h> compatible.
 */
#ifndef _ORACLE_GDTOA_ARITH_H
#define _ORACLE_GDTOA_ARITH_H

#include <locale.h>

#ifndef __LOCALE_T_DECLARED
#define __LOCALE_T_DECLARED 1
#endif

#define IEEE_LITTLE_ENDIAN

#endif
