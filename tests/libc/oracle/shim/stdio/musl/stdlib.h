/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <stdlib.h> for the musl stdio oracle: the host header plus the random-name
 * helper of tmpfile.
 */
#ifndef RL_STDIO_MUSL_STDLIB_H
#define RL_STDIO_MUSL_STDLIB_H
#include_next <stdlib.h>
#include "libc.h"
#endif
