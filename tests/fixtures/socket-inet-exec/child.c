// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K231 target fixture, exec'd half. It has the shape of `sshd-session`: it starts as a fresh
// image with an accepted socket on descriptor 7 that it never created, switches it to
// non-blocking, sends its banner and a KEXINIT-sized record first, then waits for the client's
// record with the usual `read` -> EAGAIN -> `poll(POLLIN)` loop. The socket's readiness
// doorbell belongs to the parent process, so everything here depends on the child getting a
// doorbell of its own.

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define SOCKET_FD 7
#define BANNER "SSH-2.0-RavenK231\r\n"
#define KEXINIT_BYTES 960
#define CLIENT_BYTES 2912
#define DONE "k231-done"

// A wake-up this late is the K231 lost wake-up, not a slow host: the client answers within
// milliseconds of the banner, and a missed doorbell sleeps to the full poll timeout.
#define POLL_TIMEOUT_MS 5000
#define SLOW_WAKE_MS 2500

enum {
    EXIT_OK = 0,
    EXIT_STALE = 3,
    EXIT_POLL_TIMEOUT = 4,
    EXIT_SLOW_WAKE = 5,
    EXIT_PROTOCOL = 6,
    EXIT_WRITE = 7,
    EXIT_SETUP = 8,
    EXIT_SPURIOUS = 9,
};

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

static void say_value(const char *prefix, long value, const char *suffix) {
    char line[160];
    int length = snprintf(line, sizeof(line), "%s%ld%s", prefix, value, suffix);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static long now_ms(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (long)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}

static int wait_for(short events, int timeout_ms, long *waited_ms) {
    struct pollfd waiting;
    int ready;
    long started = now_ms();
    do {
        waiting.fd = SOCKET_FD;
        waiting.events = events;
        waiting.revents = 0;
        ready = poll(&waiting, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    *waited_ms = now_ms() - started;
    return ready;
}

static int send_all(const unsigned char *data, size_t count) {
    size_t done = 0;
    while (done < count) {
        ssize_t wrote = write(SOCKET_FD, data + done, count - done);
        if (wrote > 0) {
            done += (size_t)wrote;
            continue;
        }
        if (wrote < 0 && errno == EINTR) {
            continue;
        }
        if (wrote < 0 && errno == EAGAIN) {
            long waited;
            if (wait_for(POLLOUT, POLL_TIMEOUT_MS, &waited) <= 0) {
                return -1;
            }
            continue;
        }
        return -1;
    }
    return 0;
}

int main(void) {
    static unsigned char hello[sizeof(BANNER) - 1 + KEXINIT_BYTES];
    static unsigned char client[CLIENT_BYTES];
    size_t banner_length = sizeof(BANNER) - 1;
    size_t got = 0;

    int flags = fcntl(SOCKET_FD, F_GETFL);
    if (flags < 0 || fcntl(SOCKET_FD, F_SETFL, flags | O_NONBLOCK) < 0) {
        say_value("socket-inet-exec-child: nonblock errno=", errno, "\n");
        return EXIT_SETUP;
    }

    memcpy(hello, BANNER, banner_length);
    for (size_t index = 0; index < KEXINIT_BYTES; index++) {
        hello[banner_length + index] = (unsigned char)(index % 251u);
    }
    if (send_all(hello, sizeof(hello)) < 0) {
        // The peer is already gone: a stale session left by a host retry.
        say_value("socket-inet-exec-child: hello write errno=", errno, "\n");
        return errno == EPIPE || errno == ECONNRESET ? EXIT_STALE : EXIT_WRITE;
    }
    say_value("socket-inet-exec-child: hello sent bytes=", (long)sizeof(hello), "\n");

    // `poll` reported POLLIN and the next `read` must not fail with EAGAIN. OpenSSH's
    // `ssh_packet_read_seqnr` does exactly `ppoll(POLLIN)` then `read` and treats any -1 as fatal
    // (`ssh_dispatch_run_fatal: ... Resource temporarily unavailable`), so a spurious POLLIN ends
    // the session.
    int poll_said_readable = 0;
    while (got < CLIENT_BYTES) {
        ssize_t count = read(SOCKET_FD, client + got, CLIENT_BYTES - got);
        if (count > 0) {
            got += (size_t)count;
            poll_said_readable = 0;
            continue;
        }
        if (count == 0) {
            if (got == 0) {
                say("socket-inet-exec-child: peer closed before sending\n");
                return EXIT_STALE;
            }
            say_value("socket-inet-exec-child: short client record bytes=", (long)got, "\n");
            return EXIT_PROTOCOL;
        }
        if (errno == EINTR) {
            continue;
        }
        if (errno != EAGAIN) {
            say_value("socket-inet-exec-child: read errno=", errno, "\n");
            return EXIT_PROTOCOL;
        }
        if (poll_said_readable) {
            say_value("socket-inet-exec-child: spurious POLLIN, read EAGAIN got=", (long)got, "\n");
            return EXIT_SPURIOUS;
        }
        long waited = 0;
        int ready = wait_for(POLLIN, POLL_TIMEOUT_MS, &waited);
        if (ready == 0) {
            say_value("socket-inet-exec-child: poll timeout got=", (long)got, "\n");
            return EXIT_POLL_TIMEOUT;
        }
        if (ready < 0) {
            say_value("socket-inet-exec-child: poll errno=", errno, "\n");
            return EXIT_PROTOCOL;
        }
        if (waited > SLOW_WAKE_MS) {
            say_value("socket-inet-exec-child: slow wake ms=", waited, "\n");
            return EXIT_SLOW_WAKE;
        }
        poll_said_readable = 1;
    }

    for (size_t index = 0; index < CLIENT_BYTES; index++) {
        if (client[index] != (unsigned char)(index % 251u)) {
            say_value("socket-inet-exec-child: client record differs at ", (long)index, "\n");
            return EXIT_PROTOCOL;
        }
    }
    say_value("socket-inet-exec-child: client record ok bytes=", (long)got, "\n");

    if (send_all((const unsigned char *)DONE, strlen(DONE)) < 0) {
        say_value("socket-inet-exec-child: done write errno=", errno, "\n");
        return EXIT_WRITE;
    }
    say("socket-inet-exec-child: done sent\n");
    return EXIT_OK;
}
