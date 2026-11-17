/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <stdlib.h>

int pcre2_probe(void) {
    void *memory = malloc(8);
    free(memory);
    return memory != 0;
}
