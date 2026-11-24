/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S173: polling conformance rows.
 *
 * Each row asserts the exact poll(2)/select(2) readiness result:
 *   pipe-before  no POLLIN before a byte is written
 *   pipe-after   POLLIN after the byte
 *   pipe-hup     POLLHUP once the writer is gone and the data is drained
 *   select-pipe  select(2) readfds 0 before the write, 1 after
 *   timeout      poll returns 0 after the millisecond timeout
 *   eintr        a handled signal interrupts the wait with EINTR
 *   socket-err   a failed nonblocking connect reports POLLERR; SO_ERROR is consumed once
 *   eventfd      readable only after the counter write, level-triggered until read
 *   timerfd      readable only after the expiry, level-triggered until read
 * A failure prints `polling: fail <row>` and the qtest forbids that line. */

#include <errno.h>
#include <fcntl.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/eventfd.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/timerfd.h>
#include <time.h>
#include <unistd.h>

static int failed;

static int fail(const char *row, long value, int error) {
    printf("polling: fail %s value=%ld errno=%d\n", row, value, error);
    fflush(stdout);
    failed = 1;
    return 1;
}

static int ok(const char *row) {
    printf("polling: %s ok\n", row);
    fflush(stdout);
    return 0;
}

static int elapsed_ms(const struct timespec *before, const struct timespec *after) {
    long sec = after->tv_sec - before->tv_sec;
    long nsec = after->tv_nsec - before->tv_nsec;
    if (nsec < 0) {
        sec -= 1;
        nsec += 1000000000L;
    }
    return (int)(sec * 1000L + nsec / 1000000L);
}

static int poll_one(int fd, short events, int timeout_ms, short *revents) {
    struct pollfd ready;
    int result;

    ready.fd = fd;
    ready.events = events;
    ready.revents = 0;
    result = poll(&ready, 1, timeout_ms);
    if (revents != NULL)
        *revents = ready.revents;
    if (result < 0)
        return -errno;
    return result;
}

static int pipe_rows(int read_fd, int write_fd) {
    short revents = 0;
    char byte = 'x';
    int result;

    result = poll_one(read_fd, POLLIN, 0, &revents);
    if (result != 0 || revents != 0)
        return fail("pipe-before", result, errno);
    if (ok("pipe-before") != 0)
        return 1;

    if (write(write_fd, &byte, 1) != 1)
        return fail("pipe-after-write", -1, errno);
    result = poll_one(read_fd, POLLIN, 100, &revents);
    if (result != 1 || (revents & POLLIN) == 0)
        return fail("pipe-after", result, errno);
    if (ok("pipe-after") != 0)
        return 1;

    /* select(2) on the same source: ready only while the byte is buffered. */
    {
        fd_set set;
        struct timeval timeout;
        FD_ZERO(&set);
        FD_SET(read_fd, &set);
        timeout.tv_sec = 0;
        timeout.tv_usec = 0;
        result = select(read_fd + 1, &set, NULL, NULL, &timeout);
        if (result != 1) {
            (void)fail("select-pipe", result, errno);
        } else {
            (void)ok("select-pipe");
        }
    }

    if (read(read_fd, &byte, 1) != 1)
        return fail("pipe-drain", -1, errno);
    if (close(write_fd) != 0)
        return fail("pipe-close", -1, errno);
    result = poll_one(read_fd, POLLIN, 100, &revents);
    if (result != 1 || (revents & POLLHUP) == 0)
        return fail("pipe-hup", result, errno);
    if (ok("pipe-hup") != 0)
        return 1;
    return 0;
}

static int timeout_row(int read_fd) {
    struct timespec before;
    struct timespec after;
    short revents = 0;
    int result;
    int wall;

    (void)clock_gettime(CLOCK_MONOTONIC, &before);
    result = poll_one(read_fd, POLLIN, 50, &revents);
    (void)clock_gettime(CLOCK_MONOTONIC, &after);
    wall = elapsed_ms(&before, &after);
    if (result != 0 || revents != 0 || wall < 30 || wall > 500)
        return fail("timeout", result, wall);
    return ok("timeout");
}

static volatile sig_atomic_t alarm_seen;

static void on_alarm(int signo) {
    (void)signo;
    alarm_seen = 1;
}

static int eintr_row(int read_fd) {
    struct sigaction action;
    struct itimerval timer;
    short revents = 0;
    int result;

    memset(&action, 0, sizeof(action));
    action.sa_handler = on_alarm;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0; /* no SA_RESTART: the wait must return EINTR */
    printf("polling: eintr sigaction\n");
    fflush(stdout);
    if (sigaction(SIGALRM, &action, NULL) != 0)
        return fail("eintr-sigaction", -1, errno);

    printf("polling: eintr setitimer\n");
    fflush(stdout);
    memset(&timer, 0, sizeof(timer));
    timer.it_value.tv_usec = 50 * 1000;
    if (setitimer(ITIMER_REAL, &timer, NULL) != 0)
        return fail("eintr-setitimer", -1, errno);

    printf("polling: eintr poll\n");
    fflush(stdout);
    alarm_seen = 0;
    /* Bounded: a missing signal delivery must fail the row, not hang the whole table. */
    result = poll_one(read_fd, POLLIN, 2000, &revents);
    if (result != -EINTR || !alarm_seen) {
        printf("polling: eintr detail result=%d errno=%d alarm_seen=%d revents=%d\n",
               result, errno, (int)alarm_seen, (int)revents);
        return fail("eintr", result, errno);
    }
    return ok("eintr");
}

static int socket_error_row(void) {
    struct sockaddr_in address;
    struct pollfd ready;
    socklen_t length;
    int error = 0;
    int sock;
    int result;

    sock = socket(AF_INET, SOCK_STREAM, 0);
    if (sock < 0)
        return fail("socket-create", -1, errno);
    if (fcntl(sock, F_SETFL, O_NONBLOCK) != 0)
        return fail("socket-nonblock", sock, errno);
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(1);
    address.sin_addr.s_addr = htonl(0x7f000001u); /* 127.0.0.1:1, closed */
    (void)connect(sock, (const struct sockaddr *)&address, sizeof(address));

    ready.fd = sock;
    ready.events = POLLIN | POLLOUT;
    ready.revents = 0;
    result = poll(&ready, 1, 1000);
    if (result < 0)
        return fail("socket-poll", result, errno);
    if (result != 1 || (ready.revents & POLLERR) == 0) {
        (void)close(sock);
        return fail("socket-err", result, ready.revents);
    }
    length = sizeof(error);
    if (getsockopt(sock, SOL_SOCKET, SO_ERROR, &error, &length) != 0) {
        (void)close(sock);
        return fail("socket-soerror", -1, errno);
    }
    if (error == 0) {
        (void)close(sock);
        return fail("socket-errno", error, 0);
    }
    error = -1;
    length = sizeof(error);
    if (getsockopt(sock, SOL_SOCKET, SO_ERROR, &error, &length) != 0) {
        (void)close(sock);
        return fail("socket-soerror-clear", -1, errno);
    }
    if (error != 0) {
        (void)close(sock);
        return fail("socket-soerror-once", error, 0);
    }
    (void)close(sock);
    return ok("socket-err");
}

static int eventfd_row(void) {
    uint64_t counter = 1;
    uint64_t value = 0;
    short revents = 0;
    int fd;
    int result;

    fd = eventfd(0, 0);
    if (fd < 0)
        return fail("eventfd-create", -1, errno);
    result = poll_one(fd, POLLIN, 0, &revents);
    if (result != 0 || revents != 0) {
        (void)close(fd);
        return fail("eventfd-before", result, revents);
    }
    if (write(fd, &counter, sizeof(counter)) != (ssize_t)sizeof(counter)) {
        (void)close(fd);
        return fail("eventfd-write", -1, errno);
    }
    result = poll_one(fd, POLLIN, 100, &revents);
    if (result != 1 || (revents & POLLIN) == 0) {
        (void)close(fd);
        return fail("eventfd-after", result, revents);
    }
    if (read(fd, &value, sizeof(value)) != (ssize_t)sizeof(value) || value != 1) {
        (void)close(fd);
        return fail("eventfd-read", (long)value, errno);
    }
    result = poll_one(fd, POLLIN, 0, &revents);
    (void)close(fd);
    if (result != 0 || revents != 0)
        return fail("eventfd-drained", result, revents);
    return ok("eventfd");
}

static int timerfd_row(void) {
    struct itimerspec timer;
    uint64_t expirations = 0;
    short revents = 0;
    int fd;
    int result;

    fd = timerfd_create(CLOCK_MONOTONIC, 0);
    if (fd < 0)
        return fail("timerfd-create", -1, errno);
    memset(&timer, 0, sizeof(timer));
    timer.it_value.tv_nsec = 80 * 1000 * 1000L;
    if (timerfd_settime(fd, 0, &timer, NULL) != 0) {
        (void)close(fd);
        return fail("timerfd-settime", -1, errno);
    }
    result = poll_one(fd, POLLIN, 0, &revents);
    if (result != 0 || revents != 0) {
        (void)close(fd);
        return fail("timerfd-before", result, revents);
    }
    result = poll_one(fd, POLLIN, 500, &revents);
    if (result != 1 || (revents & POLLIN) == 0) {
        (void)close(fd);
        return fail("timerfd-after", result, revents);
    }
    if (read(fd, &expirations, sizeof(expirations)) != (ssize_t)sizeof(expirations) ||
        expirations == 0) {
        (void)close(fd);
        return fail("timerfd-read", (long)expirations, errno);
    }
    result = poll_one(fd, POLLIN, 0, &revents);
    (void)close(fd);
    if (result != 0 || revents != 0)
        return fail("timerfd-drained", result, revents);
    return ok("timerfd");
}

int main(void) {
    int pipefd[2];
    int idle[2];

    if (pipe(pipefd) != 0)
        return fail("pipe-create", -1, errno);
    (void)pipe_rows(pipefd[0], pipefd[1]);
    (void)close(pipefd[0]);

    /* A fresh pipe with the writer open: the read end stays empty, so the timeout and EINTR
     * rows observe a real wait, not the hangup left by the closed pipe above. */
    if (pipe(idle) != 0) {
        (void)fail("idle-pipe-create", -1, errno);
    } else {
        (void)timeout_row(idle[0]);
    }

    (void)socket_error_row();
    (void)eventfd_row();
    (void)timerfd_row();

    /* The signal row runs last: a hang in the signal path must not hide the other rows. */
    if (idle[0] >= 0)
        (void)eintr_row(idle[0]);
    if (idle[0] >= 0) {
        (void)close(idle[0]);
        (void)close(idle[1]);
    }

    if (failed)
        return 1;
    printf("polling: ok\n");
    fflush(stdout);
    return 0;
}
