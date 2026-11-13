/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD CLEANROOM: threads are always available; the flag is initialised
 * to zero by the C-ABI shell (doc 14 3.B.5).
 */

#ifndef _SYS_SINGLE_THREADED_H
#define _SYS_SINGLE_THREADED_H

#include <sys/cdefs.h>

__BEGIN_DECLS
extern char __libc_single_threaded;
__END_DECLS

#endif /* _SYS_SINGLE_THREADED_H */
