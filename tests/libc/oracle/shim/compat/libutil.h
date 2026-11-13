/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* RavenBSD: the libutil.h prototype set for the C65 compat-humanize oracle (first-party). */
#ifndef _LIBUTIL_H_
#define _LIBUTIL_H_
#include <sys/types.h>
#include <stdint.h>
#define HN_DECIMAL 0x01
#define HN_NOSPACE 0x02
#define HN_B 0x04
#define HN_DIVISOR_1000 0x08
#define HN_IEC_PREFIXES 0x10
#define HN_GETSCALE 0x10
#define HN_AUTOSCALE 0x20
int expand_number(const char *, int64_t *);
int humanize_number(char *, size_t, int64_t, const char *, int, int);
#endif
