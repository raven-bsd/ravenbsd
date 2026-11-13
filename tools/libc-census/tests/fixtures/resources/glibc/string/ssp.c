/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

void *ssp_name(void) { return __ssp_real (ssp_name) (); }
