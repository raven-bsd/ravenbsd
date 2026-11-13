/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD oracle shim: install the oracle `locale_t` before the host
 * <ctype.h> drags in glibc's incompatible one.
 */
#ifndef _ORACLE_CTYPE_H
#define _ORACLE_CTYPE_H
#include <locale.h>
#include_next <ctype.h>
#endif
