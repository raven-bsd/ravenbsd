/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K548: poll(2) on the fd kinds the guest ssh client uses.
 *
 * The connection is an AF_UNIX socketpair end, stdout/stderr are a pipe or a pty slave.  A
 * write-only pty slave carries WRITE but not READ; the kernel EVQ source lookup must observe it
 * without returning EACCES.  The qtest expects every line below and forbids a failure line. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

static int fail(const char *what, int fd, int err) {
    printf("posix-poll: fail %s fd=%d errno=%d\n", what, fd, err);
    fflush(stdout);
    return 1;
}

/* Return poll(2) revents, or -errno on failure. */
static int poll_revents(int fd, short events, int timeout_ms) {
    struct pollfd ready;
    int result;

    ready.fd = fd;
    ready.events = events;
    ready.revents = 0;
    result = poll(&ready, 1, timeout_ms);
    if (result < 0)
        return -errno;
    return ready.revents;
}

int main(void) {
    int pair[2];
    int pipefd[2];
    int master;
    int slave;
    char *name;
    int revents;

    setvbuf(stdout, NULL, _IONBF, 0);

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair", -1, errno);
    revents = poll_revents(pair[1], POLLOUT, 1000);
    if (revents < 0)
        return fail("socket-pollout", pair[1], -revents);
    if ((revents & POLLOUT) == 0)
        return fail("socket-pollout-revents", pair[1], revents);
    if (write(pair[0], "x", 1) != 1)
        return fail("socket-write", pair[0], errno);
    revents = poll_revents(pair[1], POLLIN, 1000);
    if (revents < 0)
        return fail("socket-pollin", pair[1], -revents);
    if ((revents & POLLIN) == 0)
        return fail("socket-pollin-revents", pair[1], revents);
    printf("posix-poll: socket ok\n");

    if (pipe(pipefd) != 0)
        return fail("pipe", -1, errno);
    revents = poll_revents(pipefd[1], POLLOUT, 1000);
    if (revents < 0)
        return fail("pipe-pollout", pipefd[1], -revents);
    if ((revents & POLLOUT) == 0)
        return fail("pipe-pollout-revents", pipefd[1], revents);
    if (write(pipefd[1], "y", 1) != 1)
        return fail("pipe-write", pipefd[1], errno);
    revents = poll_revents(pipefd[0], POLLIN, 1000);
    if (revents < 0)
        return fail("pipe-pollin", pipefd[0], -revents);
    if ((revents & POLLIN) == 0)
        return fail("pipe-pollin-revents", pipefd[0], revents);
    printf("posix-poll: pipe ok\n");

    master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0)
        return fail("posix_openpt", -1, errno);
    if (grantpt(master) != 0)
        return fail("grantpt", master, errno);
    if (unlockpt(master) != 0)
        return fail("unlockpt", master, errno);
    name = ptsname(master);
    if (name == NULL)
        return fail("ptsname", master, errno);
    /* The ssh client's stdout can be a write-only pty slave: WRITE but no READ. */
    slave = open(name, O_WRONLY | O_NOCTTY);
    if (slave < 0)
        return fail("open-slave", -1, errno);
    revents = poll_revents(slave, POLLOUT, 1000);
    if (revents < 0)
        return fail("pty-pollout", slave, -revents);
    if ((revents & POLLOUT) == 0)
        return fail("pty-pollout-revents", slave, revents);
    printf("posix-poll: pty ok\n");

    close(slave);
    close(master);
    close(pipefd[0]);
    close(pipefd[1]);
    close(pair[0]);
    close(pair[1]);

    printf("posix-poll: ok\n");
    return 0;
}
