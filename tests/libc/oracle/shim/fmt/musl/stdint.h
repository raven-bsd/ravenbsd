/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <stdint.h> for the musl oracle: the host header plus the late
 * glue header, for donor files that include stdint before string.h.
 */
#ifndef RL_FMT_MUSL_STDINT_H
#define RL_FMT_MUSL_STDINT_H
#include_next <stdint.h>
#include "musl_glue.h"
#endif
