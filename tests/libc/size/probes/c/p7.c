// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P7: strftime("%c") + gmtime. Time without locale machinery (R-14).
#include <time.h>

int main(int argc, char **argv) {
    char buffer[64];
    time_t now = 1700000000;
    struct tm *tm;
    (void)argc;
    (void)argv;
    tm = gmtime(&now);
    if (tm == 0) {
        return 1;
    }
    if (strftime(buffer, sizeof buffer, "%c", tm) == 0) {
        return 2;
    }
    return 0;
}
