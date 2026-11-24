// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S198 test-only login shell for the `raven` account. The release `brush` login shell faults on
// this base before the SSH command returns (the S195 K514 handover), so the conformance runner
// cannot drive `/bin/mmap-probe`. sshd invokes this shell as
// `s198-shell -c "<command>"`; it scans that command text for the probe's `--iterations=` and
// `--mutation=private-shared` tokens and execs the probe with them. The probe rows, the kernel
// and the product under test are unchanged.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    int iterations = 1;
    int mutation = 0;
    for (int index = 1; index < argc; index++) {
        if (strstr(argv[index], "--mutation=private-shared") != 0) {
            mutation = 1;
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
    char *args[] = {
        "/bin/mmap-probe",
        iteration_arg,
        mutation ? "--mutation=private-shared" : (char *)0,
        (char *)0,
    };
    execv(args[0], args);
    _exit(127);
}
