/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: POSIX.1-2024 <iso646.h> alternative spellings for the C operators (C17 7.9).
 * The macros are not defined for C++, where the spellings are keywords.
 */
#ifndef _RAVENBSD_ISO646_H
#define _RAVENBSD_ISO646_H

#ifndef __cplusplus

#define and &&
#define and_eq &=
#define bitand &
#define bitor |
#define compl ~
#define not !
#define not_eq !=
#define or ||
#define or_eq |=
#define xor ^
#define xor_eq ^=

#endif /* __cplusplus */

#endif /* _RAVENBSD_ISO646_H */
