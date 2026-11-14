/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* Oracle shim: the SSP rename macro becomes the identity. */
#ifndef RAVEN_ORACLE_SSP_SSP_H
#define RAVEN_ORACLE_SSP_SSP_H
#define __ssp_real(x) x
#endif
