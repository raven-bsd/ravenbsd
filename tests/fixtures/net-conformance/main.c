// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K253 target fixture: one TCP listener for the packet-level conformance
// harness.  The scenario is selected entirely from the wire (the peer's
// advertised window and ACK pattern), so the guest only has to accept a
// connection and queue exactly 44 bytes.  It logs every step on the serial
// console through DEBUG_WRITE and keeps the connection open long enough for a
// persist probe, a window reopen, a retransmission and a peer FIN/RST.

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <poll.h>
#include <unistd.h>

#define PORT 23090
#define PAYLOAD "K253-zero-window-44-byte-payload-0123456789!"
#define PAYLOAD_LENGTH 44

_Static_assert(sizeof(PAYLOAD) - 1 == PAYLOAD_LENGTH,
               "the guest and harness payloads must stay byte-identical");

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

static ssize_t write_all(int fd, const void *buffer, size_t count) {
    size_t done = 0;
    while (done < count) {
        ssize_t wrote = write(fd, (const unsigned char *)buffer + done, count - done);
        if (wrote < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        done += (size_t)wrote;
    }
    return (ssize_t)done;
}

static int wait_readable(int fd, int timeout_ms) {
    struct pollfd waiting;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    int ready;
    do {
        ready = poll(&waiting, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    if (ready < 0) {
        say_value("netconf: poll errno=", errno, "\n");
        return -1;
    }
    return ready > 0 ? (int)waiting.revents : 0;
}

int main(void) {
    struct sockaddr_in address;
    int listener;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        say_value("netconf: socket errno=", errno, "\n");
        return 1;
    }
    say_value("netconf: socket fd=", listener, "\n");

    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        say_value("netconf: bind errno=", errno, "\n");
        return 1;
    }
    if (listen(listener, 8) < 0) {
        say_value("netconf: listen errno=", errno, "\n");
        return 1;
    }
    say("netconf: listening port=23090\n");

    for (;;) {
        int revents = wait_readable(listener, 1000);
        if (revents <= 0) {
            continue;
        }
        if ((revents & POLLIN) == 0) {
            say_value("netconf: listener revents=", revents, "\n");
            continue;
        }
        int connection = accept(listener, NULL, NULL);
        if (connection < 0) {
            say_value("netconf: accept errno=", errno, "\n");
            continue;
        }
        say_value("netconf: accepted fd=", connection, "\n");
        errno = 0;
        ssize_t wrote = write_all(connection, PAYLOAD, PAYLOAD_LENGTH);
        say_value("netconf: write=", (long)wrote, "");
        say_value(" errno=", errno, "\n");
        // Stay open long enough for persist probes, a window reopen, an RTO
        // retransmission, or the peer's FIN/RST.
        revents = wait_readable(connection, 12000);
        ssize_t got = -2;
        if (revents > 0) {
            char discard[64];
            got = read(connection, discard, sizeof(discard));
            say_value("netconf: peer-event=", revents, "");
            say_value(" read=", (long)got, "\n");
        } else {
            say("netconf: peer-idle\n");
        }
        // K548 fuzz: a duplicate FIN needs a short window before the application closes.
        // A reset read returns -1 and must close immediately so the 1000-connect churn
        // does not pay this delay on every connection.
        if (got == 0) {
            (void)poll(NULL, 0, 150);
        }
        (void)close(connection);
        say("netconf: closed\n");
    }
}
