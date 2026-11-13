/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: GCC's <float.h> defines FLT_ROUNDS as the constant 1, while NetBSD's
 * float.h has the dynamic C99 value. hdtoa.c's dorounding() switches on FLT_ROUNDS, so the oracle
 * maps the live FPU mode here to keep the directed-rounding differential meaningful.
 */
#ifndef _ORACLE_GDTOA_FLOAT_H
#define _ORACLE_GDTOA_FLOAT_H

#include <fenv.h>

#include_next <float.h>

#undef FLT_ROUNDS
#define FLT_ROUNDS \
	(fegetround() == FE_TOWARDZERO ? 0 : \
	 fegetround() == FE_TONEAREST ? 1 : \
	 fegetround() == FE_UPWARD ? 2 : 3)

#endif
