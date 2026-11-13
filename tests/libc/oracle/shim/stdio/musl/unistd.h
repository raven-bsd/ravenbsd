/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <unistd.h> for the musl stdio oracle: the host header plus the internal seek
 * and dup3 helpers the donor calls.
 */
#ifndef RL_STDIO_MUSL_UNISTD_H
#define RL_STDIO_MUSL_UNISTD_H
#include_next <unistd.h>
#include "syscall.h"
#endif
