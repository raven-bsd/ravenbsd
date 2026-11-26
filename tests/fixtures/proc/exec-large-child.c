/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K220: a >8 MiB static image. The read-only payload makes the ELF file exceed the old 1 MiB
 * kernel copy and the old 8 MiB spawn reader cap; the program prints the pid/argv/environment
 * inherited across the libc execve. */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static const volatile unsigned char payload[9 * 1024 * 1024]
    __attribute__((used)) = {1};

int main(int argc, char **argv) {
    const char *pid = getenv("PROC_EXEC_PID");
    const char *token = getenv("PROC_EXEC_TOKEN");
    int same = pid != NULL && atoi(pid) == (int)getpid();
    printf("exec-large: child pid=%d same=%d\n", (int)getpid(), same);
    printf("exec-large: argv=%d %s %s\n", argc, argc > 1 ? argv[1] : "-",
           argc > 2 ? argv[2] : "-");
    printf("exec-large: env=%s\n", token != NULL ? token : "missing");
    printf("exec-large: image=%zu\n", sizeof(payload));
    printf("exec-large: ok\n");
    fflush(stdout);
    return same && token != NULL && argc == 3 ? 0 : 1;
}
