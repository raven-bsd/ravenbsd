/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <string.h> for the musl oracle: the host header plus the late
 * glue header (stdio_impl.h is always included first by the donor files).
 */
#ifndef RL_FMT_MUSL_STRING_H
#define RL_FMT_MUSL_STRING_H
#include_next <string.h>
#include "musl_glue.h"
#endif
