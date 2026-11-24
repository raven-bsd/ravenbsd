/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S173: single-row EINTR isolation. No timerfd/socket rows, no SSH: install SIGALRM, arm
 * ITIMER_REAL 50 ms, poll(2000) an idle pipe; the wait must return EINTR. Markers pinpoint the
 * hang. */
#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/time.h>
#include <unistd.h>

static volatile sig_atomic_t seen;

static void on_alarm(int signo) {
    (void)signo;
    seen = 1;
}

int main(void) {
    struct sigaction action;
    struct itimerval timer;
    struct pollfd ready;
    int fds[2];
    int result;

    printf("eintr-case: start\n");
    fflush(stdout);
    if (pipe(fds) != 0) {
        printf("eintr-case: fail pipe errno=%d\n", errno);
        return 1;
    }
    memset(&action, 0, sizeof(action));
    action.sa_handler = on_alarm;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    printf("eintr-case: sigaction\n");
    fflush(stdout);
    if (sigaction(SIGALRM, &action, NULL) != 0) {
        printf("eintr-case: fail sigaction errno=%d\n", errno);
        return 1;
    }
    memset(&timer, 0, sizeof(timer));
    timer.it_value.tv_usec = 50 * 1000;
    printf("eintr-case: setitimer\n");
    fflush(stdout);
    if (setitimer(ITIMER_REAL, &timer, NULL) != 0) {
        printf("eintr-case: fail setitimer errno=%d\n", errno);
        return 1;
    }
    ready.fd = fds[0];
    ready.events = POLLIN;
    ready.revents = 0;
    seen = 0;
    printf("eintr-case: poll\n");
    fflush(stdout);
    result = poll(&ready, 1, 2000);
    printf("eintr-case: poll result=%d errno=%d seen=%d revents=%d\n",
           result, errno, (int)seen, (int)ready.revents);
    fflush(stdout);
    if (result != -1 || errno != EINTR || !seen) {
        return 1;
    }
    (void)close(fds[0]);
    (void)close(fds[1]);
    printf("eintr-case: ok\n");
    fflush(stdout);
    return 0;
}
