/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * First-party C driver prelude of the C59 variadic differential (tests/libc/va).
 *
 * This header is part of the test material of ticket C59 and is compiled at test time only;
 * it is never linked into a shipped artifact (hard rule 2 allows C in test add-on
 * directories). It exists because a variadic call needs a compile-time argument list, which
 * only a C compiler can lower exactly as a real caller would; the Rust harness generates one
 * C file with the reference (glibc) and the "ours" (our trampoline through a function
 * pointer) variant of every argument shape.
 */
#ifndef RAVEN_VA_DRIVER_PRELUDE_H
#define RAVEN_VA_DRIVER_PRELUDE_H

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* The trampoline under test, as the C driver sees it. */
typedef int (*raven_ours_fn)(char *, size_t, const char *, ...);

/* The one-named-argument recorder trampoline (`t_rec`). */
typedef int (*raven_rec_fn)(int, ...);

/* Build a double from its little-endian bit image without touching an FP register type. */
static double raven_double_from_bits(unsigned long long bits) {
    double value;
    memcpy(&value, &bits, sizeof value);
    return value;
}

/* Build an x86_64 80-bit long double image from its two halves. */
static long double raven_ld_from_bits(unsigned long long low, unsigned short high) {
    long double value;
    memset(&value, 0, sizeof value);
    memcpy(&value, &low, sizeof low);
    memcpy(((unsigned char *)&value) + 8, &high, sizeof high);
    return value;
}

#endif /* RAVEN_VA_DRIVER_PRELUDE_H */
