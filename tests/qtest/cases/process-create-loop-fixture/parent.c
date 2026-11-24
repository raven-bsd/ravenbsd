// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#include <errno.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    for (int index = 0; index < 100; index++) {
        pid_t pid = fork();
        if (pid < 0) {
            return 100 + (errno & 0x3f);
        }
        if (pid == 0) {
            _exit(0);
        }
        int status = 0;
        if (waitpid(pid, &status, 0) != pid || !WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            return 80 + (status & 0x1f);
        }
    }
    return 0;
}
