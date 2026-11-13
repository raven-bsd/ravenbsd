/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <stdlib.h> for the musl oracle: the host header plus the late
 * glue header.
 */
#ifndef RL_FMT_MUSL_STDLIB_H
#define RL_FMT_MUSL_STDLIB_H
#include_next <stdlib.h>
#include "musl_glue.h"
#endif
