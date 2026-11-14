// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P2: printf("%d\n", argc). Integer stdio core, no float (R-13).
#include <stdio.h>

int main(int argc, char **argv) {
    (void)argv;
    printf("%d\n", argc);
    return 0;
}
