/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S173: single-row eventfd readiness. No signals, no sockets. */
#include <errno.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/eventfd.h>
#include <unistd.h>

static int poll_one(int fd, int timeout_ms, short *revents) {
    struct pollfd ready;
    int result;
    ready.fd = fd;
    ready.events = POLLIN;
    ready.revents = 0;
    result = poll(&ready, 1, timeout_ms);
    *revents = ready.revents;
    return result < 0 ? -errno : result;
}

int main(void) {
    uint64_t one = 1;
    uint64_t value = 0;
    short revents = 0;
    int fd;
    int result;

    printf("eventfd-case: create\n");
    fflush(stdout);
    fd = eventfd(0, 0);
    if (fd < 0) {
        printf("eventfd-case: fail create errno=%d\n", errno);
        return 1;
    }
    result = poll_one(fd, 0, &revents);
    printf("eventfd-case: poll0 result=%d revents=%d errno=%d\n", result, (int)revents, errno);
    fflush(stdout);
    if (result != 0 || revents != 0) {
        return 1;
    }
    if (write(fd, &one, sizeof(one)) != (ssize_t)sizeof(one)) {
        printf("eventfd-case: fail write errno=%d\n", errno);
        return 1;
    }
    result = poll_one(fd, 100, &revents);
    printf("eventfd-case: poll-after result=%d revents=%d errno=%d\n", result, (int)revents, errno);
    fflush(stdout);
    if (result != 1 || (revents & POLLIN) == 0) {
        return 1;
    }
    if (read(fd, &value, sizeof(value)) != (ssize_t)sizeof(value) || value != 1) {
        printf("eventfd-case: fail read value=%lu errno=%d\n", (unsigned long)value, errno);
        return 1;
    }
    result = poll_one(fd, 0, &revents);
    if (result != 0 || revents != 0) {
        return 1;
    }
    (void)close(fd);
    printf("eventfd-case: ok\n");
    fflush(stdout);
    return 0;
}
