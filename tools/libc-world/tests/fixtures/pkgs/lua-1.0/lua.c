/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <stdio.h>

int lua_probe(void) {
    return printf("lua\n") > 0;
}
