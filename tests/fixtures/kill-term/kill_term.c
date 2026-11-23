/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * K434 pre-session probe: fork a child, kill it with SIGTERM, and require the
 * wait status to report death by that signal.
 */
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        return 1;
    }
    if (pid == 0) {
        for (;;) {
            pause();
        }
    }

    int rc = kill(pid, SIGTERM);
    int status = 0;
    pid_t waited = waitpid(pid, &status, 0);
    int signalled = WIFSIGNALED(status);
    int sig = signalled ? WTERMSIG(status) : 0;
    printf("kill-term: kill_rc=%d wait_pid=%d signalled=%d sig=%d\n",
           rc, (int)waited, signalled, sig);
    if (rc == 0 && waited == pid && signalled && sig == SIGTERM) {
        printf("kill-term: ok\n");
        return 0;
    }
    printf("kill-term: fail\n");
    return 1;
}
