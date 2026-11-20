/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * K214 closefrom: the OpenSSH closefrom fallback calls close_range(3, INT_MAX, 0). The real fd
 * tables must close only existing descriptors (or stop at the hard RLIMIT), not walk two billion
 * integers. The CLOEXEC form must be bounded the same way.
 */

#define RL_CASE "closefrom"
#include "rl_test.h"

#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <time.h>
#include <unistd.h>

/* `close_range` is exported by libc but not declared by this release's headers. */
extern int close_range(unsigned int lo, unsigned int hi, unsigned int flags);

#define CLOSE_RANGE_UNSHARE 2u
#define CLOSE_RANGE_CLOEXEC 4u

static long long now_ms(void)
{
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
        return -1;
    return (long long)ts.tv_sec * 1000LL + (long long)ts.tv_nsec / 1000000LL;
}

static void emit_ms(const char *name, long long value)
{
    char line[RL_LINE_MAX + 1];
    char *cursor = line;

    rl_copy(line, RL_CASE);
    cursor = line + rl_strlen(line);
    rl_append(&cursor, ": ");
    rl_append(&cursor, name);
    rl_append(&cursor, "-ms=");
    rl_append_uint(&cursor, (long)value);
    rl_out(line);
}

static int make_fd(int fd)
{
    return dup2(1, fd) == fd ? 0 : -1;
}

static int is_closed(int fd)
{
    errno = 0;
    return fcntl(fd, F_GETFD) == -1 && errno == EBADF;
}

static int has_cloexec(int fd)
{
    int flags = fcntl(fd, F_GETFD);
    return flags >= 0 && (flags & FD_CLOEXEC) != 0;
}

int main(void)
{
    long long started;
    long long elapsed;
    int rc;

    rl_out("closefrom: start");

    RL_CHECK("setup-closefrom", make_fd(5) == 0 && make_fd(6) == 0 && make_fd(7) == 0 &&
                                    make_fd(8) == 0);
    started = now_ms();
    closefrom(3);
    elapsed = now_ms() - started;
    RL_CHECK("closefrom-returns", elapsed >= 0 && elapsed <= 3000);
    emit_ms("closefrom", elapsed);
    RL_CHECK("closefrom-closed", is_closed(5) && is_closed(6) && is_closed(7) && is_closed(8));
    RL_CHECK("closefrom-keeps-stdio", !is_closed(0) && !is_closed(1) && !is_closed(2));

    RL_CHECK("setup-range", make_fd(5) == 0 && make_fd(9) == 0);
    started = now_ms();
    rc = close_range(3, INT_MAX, 0);
    elapsed = now_ms() - started;
    RL_CHECK("range-returns", rc == 0 && elapsed >= 0 && elapsed <= 3000);
    emit_ms("range", elapsed);
    RL_CHECK("range-closed", is_closed(5) && is_closed(9));

    RL_CHECK("setup-cloexec", make_fd(6) == 0 && make_fd(10) == 0);
    started = now_ms();
    rc = close_range(3, INT_MAX, CLOSE_RANGE_CLOEXEC);
    elapsed = now_ms() - started;
    RL_CHECK("cloexec-returns", rc == 0 && elapsed >= 0 && elapsed <= 3000);
    emit_ms("cloexec", elapsed);
    RL_CHECK("cloexec-open", !is_closed(6) && !is_closed(10));
    RL_CHECK("cloexec-set", has_cloexec(6) && has_cloexec(10));

    rc = close_range(3, INT_MAX, 0);
    RL_CHECK("cleanup-closed", rc == 0 && is_closed(6) && is_closed(10));

    errno = 0;
    rc = close_range(4, 2, 0);
    RL_CHECK("einval-reversed", rc == -1 && errno == EINVAL);
    errno = 0;
    rc = close_range(3, 9, CLOSE_RANGE_UNSHARE);
    RL_CHECK("einval-unshare", rc == -1 && errno == EINVAL);
    errno = 0;
    rc = close_range(3, 9, 0x100);
    RL_CHECK("einval-unknown", rc == -1 && errno == EINVAL);

    return rl_finish();
}
