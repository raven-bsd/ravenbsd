// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P5: getaddrinfo(argv[1], "80", 0, &r). Resolver graph (3.A.4.4).
#include <netdb.h>

int main(int argc, char **argv) {
    struct addrinfo *result = 0;
    const char *node = argc > 1 ? argv[1] : "127.0.0.1";
    if (getaddrinfo(node, "80", 0, &result) != 0) {
        return 1;
    }
    freeaddrinfo(result);
    return 0;
}
