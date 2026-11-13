/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the NetBSD namespace aliases are not needed in a single-object diff
 * family; names must stay exactly the donor's for the export list. NetBSD's <stdlib.h> brings
 * in uint32_t through <sys/types.h>; the host <stdlib.h> does not, so include it here.
 */
#ifndef _ORACLE_NAMESPACE_H
#define _ORACLE_NAMESPACE_H
#include <stdint.h>
#endif
