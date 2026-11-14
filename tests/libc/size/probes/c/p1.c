// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P1: write(1, "hi\n", 3), no stdio (R-15).
#include <unistd.h>

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    return write(1, "hi\n", 3) == 3 ? 0 : 1;
}
