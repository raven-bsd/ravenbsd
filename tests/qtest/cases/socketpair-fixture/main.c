// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K206 target fixture: exercise the real libc socketpair(AF_UNIX) path over the kernel duplex
// endpoint pair (K208). It prints a single diagnostic line per check on the RavenBSD debug
// console, so the qtest can match it without a tty.

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
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

static void say_errno(const char *what, int error) {
    char line[160];
    int length = snprintf(line, sizeof(line), "socketpair: %s errno=%d\n", what, error);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int fail(const char *what) {
    say_errno(what, errno);
    say("socketpair: fail\n");
    return 1;
}

static int close_pair(void) {
    int sv[2];

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("close socketpair");
    }
    if (close(sv[0]) != 0) {
        return fail("close first");
    }
    if (close(sv[1]) != 0) {
        return fail("close second");
    }
    say("socketpair: close ok\n");
    return 0;
}

static int stream_pair(void) {
    int sv[2];
    char buffer[16];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("socketpair");
    }

    if (write(sv[0], "ping", 4) != 4) {
        return fail("a-to-b write");
    }
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 4 || memcmp(buffer, "ping", 4) != 0) {
        return fail("a-to-b read");
    }
    say("socketpair: a-to-b ok\n");

    if (write(sv[1], "pong", 4) != 4) {
        return fail("b-to-a write");
    }
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != 4 || memcmp(buffer, "pong", 4) != 0) {
        return fail("b-to-a read");
    }
    say("socketpair: b-to-a ok\n");

    if (shutdown(sv[0], SHUT_WR) != 0) {
        return fail("shutdown write");
    }
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("peer eof");
    }
    say("socketpair: eof ok\n");

    if (close(sv[0]) != 0) {
        return fail("close a");
    }
    count = send(sv[1], "x", 1, MSG_NOSIGNAL);
    if (count != -1 || errno != EPIPE) {
        return fail("peer close");
    }
    say("socketpair: peer-close ok\n");

    if (close(sv[1]) != 0) {
        return fail("close b");
    }
    return 0;
}

static int shutdown_read_pair(void) {
    int sv[2];
    char buffer[4];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("shutdown-read socketpair");
    }
    if (shutdown(sv[0], SHUT_RD) != 0) {
        return fail("shutdown read");
    }
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("shutdown read eof");
    }
    count = send(sv[1], "x", 1, MSG_NOSIGNAL);
    if (count != -1 || errno != EPIPE) {
        return fail("shutdown read peer epipe");
    }
    say("socketpair: shutdown-read ok\n");
    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}

static int shutdown_both_pair(void) {
    int sv[2];
    char buffer[4];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("shutdown-both socketpair");
    }
    if (shutdown(sv[0], SHUT_RDWR) != 0) {
        return fail("shutdown both");
    }
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("shutdown both eof");
    }
    count = send(sv[0], "x", 1, MSG_NOSIGNAL);
    if (count != -1 || errno != EPIPE) {
        return fail("shutdown both epipe");
    }
    say("socketpair: shutdown-both ok\n");
    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}

static int datagram_pair(void) {
    int sv[2];
    char buffer[16];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_DGRAM, 0, sv) != 0) {
        return fail("dgram socketpair");
    }
    if (send(sv[0], "one", 3, 0) != 3 || send(sv[0], "two", 3, 0) != 3) {
        return fail("dgram write");
    }
    count = recv(sv[1], buffer, sizeof(buffer), 0);
    if (count != 3 || memcmp(buffer, "one", 3) != 0) {
        return fail("dgram first");
    }
    count = recv(sv[1], buffer, sizeof(buffer), 0);
    if (count != 3 || memcmp(buffer, "two", 3) != 0) {
        return fail("dgram second");
    }
    say("socketpair: dgram ok\n");
    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}

int main(void) {
    if (datagram_pair() != 0) {
        return 1;
    }
    if (shutdown_read_pair() != 0) {
        return 1;
    }
    if (shutdown_both_pair() != 0) {
        return 1;
    }
    if (close_pair() != 0) {
        return 1;
    }
    if (stream_pair() != 0) {
        return 1;
    }
    say("socketpair: ok\n");
    return 0;
}
