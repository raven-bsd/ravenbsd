// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K408 fork+exec child: write the "EXEC" marker to the inherited accepted-socket descriptor.

#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
#include <unistd.h>

static int write_all(int fd, const char *data, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t wrote = write(fd, data + done, length - done);
        if (wrote < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        done += (size_t)wrote;
    }
    return 0;
}

int main(int argc, char **argv) {
    int descriptor;

    if (argc < 2) {
        return 2;
    }
    descriptor = atoi(argv[1]);
    if (descriptor < 0) {
        return 3;
    }
    return write_all(descriptor, "EXEC", 4) == 0 ? 0 : 4;
}
