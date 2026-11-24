// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S177/S195 test-only login shell. The release brush login shell currently null-writes at
// rip=0x4e5bbf (K514 handover), so the SSH command never returns. This wrapper runs the
// unprivileged mmap probe directly, one or `--iterations=N` times, and stops on the first failed
// probe. The probe rows and the product under test are unchanged.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static int run_probe(int mutation) {
    pid_t pid = fork();
    if (pid < 0) {
        return 1;
    }
    if (pid == 0) {
        if (mutation) {
            char *args[] = {"/bin/mmap-probe", "--mutation=private-shared", (char *)0};
            execv(args[0], args);
        } else {
            char *args[] = {"/bin/mmap-probe", (char *)0};
            execv(args[0], args);
        }
        _exit(127);
    }
    int status = 0;
    if (waitpid(pid, &status, 0) != pid) {
        return 1;
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    return 1;
}

int main(int argc, char **argv) {
    int iterations = 1;
    int mutation = 0;
    for (int index = 1; index < argc; index++) {
        if (strncmp(argv[index], "--iterations=", 13) == 0) {
            iterations = atoi(argv[index] + 13);
        }
        if (strcmp(argv[index], "--mutation=private-shared") == 0) {
            mutation = 1;
        }
    }
    if (iterations < 1) {
        iterations = 1;
    }
    for (int index = 0; index < iterations; index++) {
        char marker[64];
        int length = snprintf(marker, sizeof(marker), "S177 iteration %d/%d\n",
                              index + 1, iterations);
        if (length > 0 && (size_t)length < sizeof(marker)) {
            (void)!write(2, marker, (size_t)length);
        }
        int code = run_probe(mutation);
        if (code != 0) {
            return code;
        }
        if (mutation) {
            break;
        }
    }
    return 0;
}
