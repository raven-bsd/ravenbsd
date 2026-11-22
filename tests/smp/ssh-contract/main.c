// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K435: kernel-side SSH session contract checks. The fixture uses a real libc
// socketpair(AF_UNIX) over the kernel duplex endpoint pair and prints one marker per row on the
// kernel debug console: poll/select level readiness, partial read/EAGAIN, partial write/EAGAIN,
// and shared readiness across dup(). The exec/fork/PTY rows are covered by the K206/K222/K408
// and libc-forkpty qtests driven by tests/smp/ssh-contract/run.sh.

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <unistd.h>

// raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written.
static long raven_debug_write(const char *bytes, size_t len) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t length asm("rsi") = len;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(length)
                     : "rcx", "r11", "memory");
    return number;
}

static void say(const char *text) {
    (void)raven_debug_write(text, strlen(text));
}

static int fail(const char *what) {
    char line[160];
    int length = snprintf(line, sizeof(line), "ssh-contract: fail %s errno=%d\n", what, errno);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    say("ssh-contract: fail\n");
    return 1;
}

static int poll_one(int fd, short events, int timeout_ms) {
    struct pollfd pfd;
    int ready;

    // The SSH poll path always uses a nonzero timeout. The libc non-scheme descriptor
    // zero-timeout query is a separate libc gap (K434); 1 ms still measures the current level
    // without waiting for a state change.
    if (timeout_ms == 0) {
        timeout_ms = 1;
    }
    pfd.fd = fd;
    pfd.events = events;
    pfd.revents = 0;
    do {
        ready = poll(&pfd, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    return ready < 0 ? -1 : pfd.revents;
}

static int select_read(int fd, int timeout_ms) {
    fd_set read_set;
    struct timeval timeout;
    int ready;

    if (timeout_ms == 0) {
        timeout_ms = 1;
    }
    FD_ZERO(&read_set);
    FD_SET(fd, &read_set);
    timeout.tv_sec = timeout_ms / 1000;
    timeout.tv_usec = (timeout_ms % 1000) * 1000;
    do {
        ready = select(fd + 1, &read_set, NULL, NULL, &timeout);
    } while (ready < 0 && errno == EINTR);
    return ready;
}

static int check_poll(void) {
    int sv[2];
    char byte = 'A';
    int revents;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("poll socketpair");
    }
    // Empty socket: write end is writable now; read end is not readable.
    revents = poll_one(sv[1], POLLOUT, 0);
    if (revents < 0 || (revents & POLLOUT) == 0) {
        return fail("poll empty write-ready");
    }
    revents = poll_one(sv[0], POLLIN, 0);
    if (revents != 0) {
        return fail("poll empty read-ready");
    }
    if (select_read(sv[0], 0) != 0) {
        return fail("select empty read-ready");
    }
    if (write(sv[1], &byte, 1) != 1) {
        return fail("poll write");
    }
    revents = poll_one(sv[0], POLLIN, 0);
    if (revents < 0 || (revents & POLLIN) == 0) {
        return fail("poll read-ready");
    }
    if (select_read(sv[0], 0) != 1) {
        return fail("select read-ready");
    }
    if (read(sv[0], &byte, 1) != 1) {
        return fail("poll drain");
    }
    // Level-triggered: after the drain the same descriptor is no longer readable.
    revents = poll_one(sv[0], POLLIN, 0);
    if (revents != 0) {
        return fail("poll level after drain");
    }
    if (select_read(sv[0], 0) != 0) {
        return fail("select level after drain");
    }
    if (close(sv[1]) != 0) {
        return fail("poll close writer");
    }
    revents = poll_one(sv[0], POLLIN | POLLHUP, 0);
    if (revents < 0 || (revents & POLLHUP) == 0) {
        return fail("poll hup");
    }
    if (close(sv[0]) != 0) {
        return fail("poll close reader");
    }
    say("ssh-contract: poll ok\n");
    return 0;
}

static int check_read(void) {
    int sv[2];
    unsigned char payload[100];
    unsigned char buffer[100];
    size_t index;
    ssize_t count;

    for (index = 0; index < sizeof(payload); index++) {
        payload[index] = (unsigned char)(index + 1);
    }
    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, sv) != 0) {
        return fail("read socketpair");
    }
    say("ssh-contract: read stage=socketpair\n");
    if (write(sv[1], payload, sizeof(payload)) != (ssize_t)sizeof(payload)) {
        return fail("read write");
    }
    say("ssh-contract: read stage=write\n");
    count = read(sv[0], buffer, 10);
    if (count != 10 || memcmp(buffer, payload, 10) != 0) {
        return fail("read partial first");
    }
    say("ssh-contract: read stage=partial1\n");
    count = read(sv[0], buffer, 10);
    if (count != 10 || memcmp(buffer, payload + 10, 10) != 0) {
        return fail("read partial second");
    }
    say("ssh-contract: read stage=partial2\n");
    if (poll_one(sv[0], POLLIN, 0) == 0) {
        return fail("read remaining poll");
    }
    say("ssh-contract: read stage=poll-remaining\n");
    count = read(sv[0], buffer, 80);
    if (count != 80 || memcmp(buffer, payload + 20, 80) != 0) {
        return fail("read remainder");
    }
    say("ssh-contract: read stage=remainder\n");
    if (poll_one(sv[0], POLLIN, 0) != 0) {
        return fail("read level after drain");
    }
    say("ssh-contract: read stage=level-drained\n");
    if (fcntl(sv[0], F_SETFL, O_NONBLOCK) != 0) {
        return fail("read nonblock");
    }
    say("ssh-contract: read stage=nonblock\n");
    errno = 0;
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != -1 || errno != EAGAIN) {
        return fail("read empty EAGAIN");
    }
    say("ssh-contract: read stage=eagain\n");
    if (close(sv[0]) != 0 || close(sv[1]) != 0) {
        return fail("read close");
    }
    say("ssh-contract: read ok\n");
    return 0;
}

static unsigned char big_write[128 * 1024];

static int check_write(void) {
    int sv[2];
    ssize_t first;
    ssize_t second;
    ssize_t want;
    ssize_t got;
    unsigned char scratch[4096];

    if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0, sv) != 0) {
        return fail("write socketpair");
    }
    say("ssh-contract: write stage=socketpair\n");
    if (fcntl(sv[1], F_SETFL, O_NONBLOCK) != 0) {
        return fail("write nonblock");
    }
    say("ssh-contract: write stage=nonblock\n");
    memset(big_write, 0x5a, sizeof(big_write));
    first = write(sv[1], big_write, sizeof(big_write));
    say("ssh-contract: write stage=first\n");
    if (first <= 0 || first >= (ssize_t)sizeof(big_write)) {
        return fail("write partial");
    }
    errno = 0;
    second = write(sv[1], big_write, 4096);
    say("ssh-contract: write stage=eagain-check\n");
    if (second != -1 || errno != EAGAIN) {
        return fail("write full EAGAIN");
    }
    // Drain exactly the bytes accepted by the partial write, then the writable level returns.
    want = first;
    while (want > 0) {
        got = read(sv[0], scratch, want < (ssize_t)sizeof(scratch) ? (size_t)want : sizeof(scratch));
        if (got <= 0) {
            return fail("write drain");
        }
        want -= got;
    }
    say("ssh-contract: write stage=drained\n");
    if (write(sv[1], big_write, 4096) != 4096) {
        return fail("write after drain");
    }
    say("ssh-contract: write stage=after-drain\n");
    if (close(sv[0]) != 0 || close(sv[1]) != 0) {
        return fail("write close");
    }
    say("ssh-contract: write ok\n");
    return 0;
}

static int check_dup(void) {
    int sv[2];
    int duplicate;
    char byte = 'D';

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("dup socketpair");
    }
    duplicate = dup(sv[0]);
    if (duplicate < 0) {
        return fail("dup");
    }
    say("ssh-contract: dup stage=dup\n");
    if (poll_one(sv[0], POLLIN, 0) != 0 || poll_one(duplicate, POLLIN, 0) != 0) {
        return fail("dup empty readiness");
    }
    say("ssh-contract: dup stage=empty\n");
    if (write(sv[1], &byte, 1) != 1) {
        return fail("dup write");
    }
    if (poll_one(sv[0], POLLIN, 0) == 0 || poll_one(duplicate, POLLIN, 0) == 0) {
        return fail("dup shared readiness");
    }
    say("ssh-contract: dup stage=shared\n");
    if (read(duplicate, &byte, 1) != 1) {
        return fail("dup read");
    }
    // The duplicate and the original share one open file description, so the drain is visible
    // through both descriptors.
    if (poll_one(sv[0], POLLIN, 0) != 0 || poll_one(duplicate, POLLIN, 0) != 0) {
        return fail("dup shared drain");
    }
    say("ssh-contract: dup stage=drained\n");
    if (close(duplicate) != 0 || close(sv[0]) != 0 || close(sv[1]) != 0) {
        return fail("dup close");
    }
    say("ssh-contract: dup ok\n");
    return 0;
}

int main(void) {
    if (check_poll() != 0 || check_read() != 0 || check_write() != 0 || check_dup() != 0) {
        return 1;
    }
    say("ssh-contract: ok\n");
    return 0;
}
