/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * The three ATF_DEFS_* macros, as GCC attributes. Installed as <out>/include/atf-c/defs.h.
 */
#ifndef ATF_C_DEFS_H
#define ATF_C_DEFS_H

#define ATF_DEFS_ATTRIBUTE_FORMAT_PRINTF(a, b) __attribute__((__format__(__printf__, a, b)))
#define ATF_DEFS_ATTRIBUTE_NORETURN __attribute__((__noreturn__))
#define ATF_DEFS_ATTRIBUTE_UNUSED __attribute__((__unused__))

#endif /* !defined(ATF_C_DEFS_H) */
