/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <string.h>

extern void strlcpy(char *destination, const char *source, unsigned long size);

int zlib_probe(void) {
    char buffer[8];
    strlcpy(buffer, "x", sizeof buffer);
    return buffer[0];
}
