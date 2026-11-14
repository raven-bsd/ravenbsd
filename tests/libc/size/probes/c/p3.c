// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P3: printf("%f\n", argc * 1.5). The float plug-in must be pulled (R-13).
#include <stdio.h>

int main(int argc, char **argv) {
    (void)argv;
    printf("%f\n", argc * 1.5);
    return 0;
}
