// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S198 test-only login shell for the `raven` account. The release `brush` login shell faults on
// this base before the SSH command returns (the S195 K514 handover), so the conformance runner
// cannot drive `/bin/mmap-probe`. sshd invokes this shell as
// `s198-shell -c "<command>"`; it scans that command text for the probe's `--iterations=`,
// `--mutation=private-shared` and `--sibling-exit` tokens and execs the probe with them. The
// probe rows, the kernel and the product under test are unchanged.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    int iterations = 1;
    int mutation = 0;
    int sibling_exit = 0;
    for (int index = 1; index < argc; index++) {
        if (strstr(argv[index], "--mutation=private-shared") != 0) {
            mutation = 1;
        }
        if (strstr(argv[index], "--sibling-exit") != 0) {
            sibling_exit = 1;
        }
        const char *value = strstr(argv[index], "--iterations=");
        if (value != 0) {
            iterations = atoi(value + 13);
        }
    }
    if (iterations < 1) {
        iterations = 1;
    }
    char iteration_arg[32];
    (void)snprintf(iteration_arg, sizeof iteration_arg, "--iterations=%d", iterations);
    char *args[5];
    int count = 0;
    args[count++] = "/bin/mmap-probe";
    args[count++] = iteration_arg;
    if (mutation) {
        args[count++] = "--mutation=private-shared";
    }
    if (sibling_exit) {
        args[count++] = "--sibling-exit";
    }
    args[count] = (char *)0;
    execv(args[0], args);
    _exit(127);
}
