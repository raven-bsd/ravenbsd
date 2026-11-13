/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: keep the host <time.h> instead of the donor header.
 */
#ifndef _ORACLE_TIME_H
#define _ORACLE_TIME_H
#include "tzfile.h"
#include_next <time.h>
#endif
