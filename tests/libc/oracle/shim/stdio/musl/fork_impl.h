/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement "fork_impl.h" for the musl stdio oracle: only the open-file-list lock pointer
 * declaration is used by ofl.c.
 */
#ifndef RL_STDIO_MUSL_FORK_IMPL_H
#define RL_STDIO_MUSL_FORK_IMPL_H

extern volatile int *const __stdio_ofl_lockptr;

#endif
