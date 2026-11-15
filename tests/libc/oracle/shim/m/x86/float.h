/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * FreeBSD's sys/amd64/include/float.h includes <x86/float.h>; the host's float.h already
 * describes the x86-64 long double (64-bit significand, 15-bit exponent), so the shim forwards
 * to it.
 */
#ifndef RL_M_X86_FLOAT_H
#define RL_M_X86_FLOAT_H
#include <float.h>
#endif
