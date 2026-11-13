/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Minimal replacement for musl's private locale header, written from scratch (guide 6.2).
 * The oracle does not translate messages; `LCTRANS`/`LCTRANS_CUR` are the identity and the
 * locale argument is ignored.
 */
#ifndef _LOCALE_IMPL_H_
#define _LOCALE_IMPL_H_

#ifndef __cplusplus
typedef struct __locale_struct *locale_t;
#endif

#define CURRENT_LOCALE ((locale_t)0)
#define LCTRANS(msg, cat, loc) (msg)
#define LCTRANS_CUR(msg) (msg)

#endif
