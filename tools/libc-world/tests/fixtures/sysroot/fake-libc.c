/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

unsigned long strlen(const char *text) {
    unsigned long length = 0;
    while (text[length] != 0) {
        length += 1;
    }
    return length;
}
