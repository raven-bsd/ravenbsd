/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD CLEANROOM: the unprefixed endian.h of doc 14 3.B.3.c.
 * Behaviour read from freebsd:sys/sys/endian.h (BSD-2-Clause); no text copied.
 */

#ifndef _ENDIAN_H_
#define _ENDIAN_H_

#include <sys/cdefs.h>
#include <sys/endian.h>

/* RavenBSD: byte-order values fixed by doc 14 3.B.3.c. */
#ifndef __LITTLE_ENDIAN
#define __LITTLE_ENDIAN 1234
#endif
#ifndef __BIG_ENDIAN
#define __BIG_ENDIAN 4321
#endif
#ifndef __PDP_ENDIAN
#define __PDP_ENDIAN 3412
#endif
#ifndef __BYTE_ORDER
#define __BYTE_ORDER __LITTLE_ENDIAN
#endif

#if __BSD_VISIBLE
#ifndef LITTLE_ENDIAN
#define LITTLE_ENDIAN __LITTLE_ENDIAN
#endif
#ifndef BIG_ENDIAN
#define BIG_ENDIAN __BIG_ENDIAN
#endif
#ifndef PDP_ENDIAN
#define PDP_ENDIAN __PDP_ENDIAN
#endif
#ifndef BYTE_ORDER
#define BYTE_ORDER __BYTE_ORDER
#endif
#endif

#endif /* _ENDIAN_H_ */
