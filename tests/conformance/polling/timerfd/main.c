/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S173: single-row timerfd isolation. No signal rows, no SSH: create a timerfd, arm 80 ms,
 * poll(0) must be 0, poll(500) must report POLLIN, the read must consume the expiry and the
 * drained fd must poll 0 again. Markers pinpoint a hang. */

#include <errno.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/timerfd.h>
#include <time.h>
#include <unistd.h>

static int poll_one(int fd, int timeout_ms, short *revents) {
    struct pollfd ready;
    int result;
    ready.fd = fd;
    ready.events = POLLIN;
    ready.revents = 0;
    result = poll(&ready, 1, timeout_ms);
    *revents = ready.revents;
    if (result < 0) {
        return -errno;
    }
    return result;
}

int main(void) {
    struct itimerspec timer;
    uint64_t expirations = 0;
    short revents = 0;
    int fd;
    int result;

    printf("timerfd-case: create\n");
    fflush(stdout);
    fd = timerfd_create(CLOCK_MONOTONIC, 0);
    if (fd < 0) {
        printf("timerfd-case: fail create errno=%d\n", errno);
        return 1;
    }
    memset(&timer, 0, sizeof(timer));
    timer.it_value.tv_nsec = 80 * 1000 * 1000L;
    printf("timerfd-case: settime\n");
    fflush(stdout);
    if (timerfd_settime(fd, 0, &timer, NULL) != 0) {
        printf("timerfd-case: fail settime errno=%d\n", errno);
        return 1;
    }
    printf("timerfd-case: poll0\n");
    fflush(stdout);
    result = poll_one(fd, 0, &revents);
    printf("timerfd-case: poll0 result=%d revents=%d errno=%d\n", result, (int)revents, errno);
    fflush(stdout);
    if (result != 0 || revents != 0) {
        return 1;
    }
    printf("timerfd-case: poll-expiry\n");
    fflush(stdout);
    result = poll_one(fd, 500, &revents);
    printf("timerfd-case: poll-expiry result=%d revents=%d errno=%d\n", result, (int)revents, errno);
    fflush(stdout);
    if (result != 1 || (revents & POLLIN) == 0) {
        return 1;
    }
    if (read(fd, &expirations, sizeof(expirations)) != (ssize_t)sizeof(expirations) ||
        expirations == 0) {
        printf("timerfd-case: fail read expirations=%lu errno=%d\n",
               (unsigned long)expirations, errno);
        return 1;
    }
    printf("timerfd-case: poll-drained\n");
    fflush(stdout);
    result = poll_one(fd, 0, &revents);
    if (result != 0 || revents != 0) {
        return 1;
    }
    (void)close(fd);
    printf("timerfd-case: ok\n");
    fflush(stdout);
    return 0;
}
