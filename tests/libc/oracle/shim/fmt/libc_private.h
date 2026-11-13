/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement libc_private.h for the FreeBSD printf oracle, written from
 * scratch: the locking hooks are no-ops because the oracle is single threaded.
 */
#ifndef RL_FMT_LIBC_PRIVATE_H
#define RL_FMT_LIBC_PRIVATE_H

#define FLOCKFILE_CANCELSAFE(fp) ((void)0)
#define FUNLOCKFILE_CANCELSAFE(fp) ((void)0)
#define FLOCKFILE(fp) ((void)0)
#define FUNLOCKFILE(fp) ((void)0)

#endif
