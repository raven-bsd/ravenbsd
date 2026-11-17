/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <stdlib.h>

int toy_probe(void) {
    return malloc(1) != 0;
}
