/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <math.h> for the FreeBSD printf oracle: the host header plus the
 * late glue header, which is included here because printfcommon.h includes
 * <math.h> after local.h and fvwrite.h.
 */
#ifndef RL_FMT_MATH_H
#define RL_FMT_MATH_H
#include_next <math.h>
#include "fmt_glue.h"
#endif
