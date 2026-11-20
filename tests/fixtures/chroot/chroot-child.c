/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K223: after exec the child keeps the parent's per-process root. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

int main(void) {
    int inside = open("/inside", O_RDONLY) >= 0;
    int outside = open("/outside", O_RDONLY) >= 0;
    printf("chroot-child: inside=%d outside=%d\n", inside, outside);
    fflush(stdout);
    return inside && !outside ? 0 : 1;
}
