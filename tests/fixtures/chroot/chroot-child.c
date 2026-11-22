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
    int symlink = open("/good", O_RDONLY) >= 0;
    int escape = open("/escape", O_RDONLY) >= 0;
    printf(
        "chroot-child: inside=%d outside=%d symlink=%d escape=%d\n",
        inside,
        outside,
        symlink,
        escape
    );
    fflush(stdout);
    return inside && !outside && symlink && !escape ? 0 : 1;
}
