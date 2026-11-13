/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Minimal replacement for FreeBSD's private libc header, written from scratch (guide 6.2).
 * `memset_s.c` only needs the constraint-handler declaration; RavenBSD does not ship Annex K
 * handlers (RD-12), so the oracle build makes it a no-op.
 */
#ifndef _LIBC_PRIVATE_H_
#define _LIBC_PRIVATE_H_

typedef int errno_t;
typedef size_t rsize_t;
#ifndef RSIZE_MAX
#define RSIZE_MAX (SIZE_MAX >> 1)
#endif

#define __throw_constraint_handler_s(msg, err) ((void)0)

#endif
