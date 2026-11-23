// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K434 S8 test-only `tty`: the release profile has no tty utility. This fixture prints the POSIX
// ttyname of stdin so `ssh -tt ... tty` can assert /dev/pts/N. It is test tooling, not base code.

#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main(void) {
    char *name = ttyname(STDIN_FILENO);

    if (name == NULL) {
        fprintf(stderr, "tty: not a tty: %s\n", strerror(errno));
        return 1;
    }
    printf("%s\n", name);
    return 0;
}
