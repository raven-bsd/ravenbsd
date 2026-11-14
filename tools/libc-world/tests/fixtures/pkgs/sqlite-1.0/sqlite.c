/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <stdio.h>
#include <stdlib.h>

int sqlite_probe(void) {
    void *memory = malloc(16);
    if (memory == 0) {
        return 0;
    }
    free(memory);
    return 1;
}
