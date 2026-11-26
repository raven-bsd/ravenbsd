/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K548/PG-7: an asynchronous SIGCHLD handler.
 *
 * The first child exits while SIGCHLD is blocked; the pending signal is delivered when the mask
 * is restored. The second child exits while SIGCHLD is unblocked; the handler must run during the
 * blocking wait. The handler is the kernel's SIG_SETUP entry, not a libc-only direct call.
 *
 * The wait path deliberately accepts EINTR and retries: the libc must return the interrupted
 * wait, not suppress it because a handler exists. The second child sleeps briefly so the parent
 * is blocked in waitpid before the child exits; at least one EINTR is required there. */

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile sig_atomic_t caught;

static void on_sigchld(int sig) {
    (void)sig;
    caught = 1;
}

static int reap(pid_t child, int *status, int *eintr) {
    pid_t waited;
    for (;;) {
        waited = waitpid(child, status, 0);
        if (waited == child)
            return 0;
        if (waited < 0 && errno == EINTR) {
            if (eintr != NULL)
                (*eintr)++;
            continue;
        }
        printf("libc-signal-chld: fail waitpid=%d errno=%d\n", (int)waited, errno);
        return -1;
    }
}

int main(void) {
    struct sigaction action;
    sigset_t block;
    sigset_t old;
    pid_t child;
    int status = 0;
    int spins;
    int wait_eintr = 0;

    printf("libc-signal-chld: start\n");
    (void)signal(SIGPIPE, SIG_IGN);

    action.sa_handler = on_sigchld;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    if (sigaction(SIGCHLD, &action, NULL) != 0) {
        printf("libc-signal-chld: fail sigaction errno=%d\n", errno);
        return 1;
    }
    printf("libc-signal-chld: sigaction ok\n");

    sigemptyset(&block);
    sigaddset(&block, SIGCHLD);
    if (sigprocmask(SIG_BLOCK, &block, &old) != 0) {
        printf("libc-signal-chld: fail sigprocmask errno=%d\n", errno);
        return 1;
    }
    child = fork();
    if (child < 0) {
        printf("libc-signal-chld: fail fork errno=%d\n", errno);
        return 1;
    }
    if (child == 0)
        _exit(0);
    if (reap(child, &status, NULL) != 0)
        return 1;
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("libc-signal-chld: fail child status=%d\n", status);
        return 1;
    }
    printf("libc-signal-chld: blocked child reaped\n");

    if (sigprocmask(SIG_SETMASK, &old, NULL) != 0) {
        printf("libc-signal-chld: fail unblock errno=%d\n", errno);
        return 1;
    }
    for (spins = 0; spins < 1000 && !caught; spins++)
        (void)usleep(1000);
    if (!caught) {
        printf("libc-signal-chld: fail pending handler missed\n");
        return 1;
    }
    printf("libc-signal-chld: pending handler ran\n");

    caught = 0;
    child = fork();
    if (child < 0) {
        printf("libc-signal-chld: fail fork2 errno=%d\n", errno);
        return 1;
    }
    if (child == 0) {
        /* Let the parent reach the blocking waitpid before this child exits. */
        (void)usleep(100000);
        _exit(0);
    }
    if (reap(child, &status, &wait_eintr) != 0)
        return 1;
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        printf("libc-signal-chld: fail status2=%d\n", status);
        return 1;
    }
    if (wait_eintr == 0) {
        printf("libc-signal-chld: fail wait was not interrupted\n");
        return 1;
    }
    printf("libc-signal-chld: wait eintr ok\n");
    for (spins = 0; spins < 1000 && !caught; spins++)
        (void)usleep(1000);
    if (!caught) {
        printf("libc-signal-chld: fail wait handler missed\n");
        return 1;
    }
    printf("libc-signal-chld: wait handler ran\n");
    printf("libc-signal-chld: ok\n");
    return 0;
}
