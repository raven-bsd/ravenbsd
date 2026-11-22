// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K230 target fixture: the AF_INET socket path. The program creates a stream socket through
// ravenlibc, binds it to the QEMU user-network port, listens, accepts one hostfwd connection,
// reads the host payload, echoes it, and waits for the host acknowledgement. No fork/exec: the
// accepted fd path is K225's.

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <poll.h>
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

static void say_line(const char *prefix, long value, const char *suffix) {
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

static int fail(const char *what) {
    char line[160];
    int length = snprintf(line, sizeof(line), "socket-inet: %s errno=%d\n", what, errno);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    say("socket-inet: fail\n");
    return 1;
}

static ssize_t read_exact(int fd, void *buffer, size_t count) {
    size_t done = 0;
    while (done < count) {
        ssize_t got = read(fd, (unsigned char *)buffer + done, count - done);
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (got == 0) {
            break;
        }
        done += (size_t)got;
    }
    return (ssize_t)done;
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
    return ready > 0 && (waiting.revents & POLLIN) != 0;
}

static void say_peer(const struct sockaddr_in *peer) {
    char line[192];
    uint32_t address = ntohl(peer->sin_addr.s_addr);
    int length = snprintf(
        line,
        sizeof(line),
        "socket-inet: accepted peer=%u.%u.%u.%u:%u\n",
        (unsigned)((address >> 24) & 0xffu),
        (unsigned)((address >> 16) & 0xffu),
        (unsigned)((address >> 8) & 0xffu),
        (unsigned)(address & 0xffu),
        (unsigned)ntohs(peer->sin_port));
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

#define PORT 23080
#define PAYLOAD "socket-inet-payload"

int main(void) {
    struct sockaddr_in address;
    struct sockaddr_in peer;
    socklen_t peer_length = sizeof(peer);
    char payload[64];
    char ack[64];
    const char *expected = PAYLOAD;
    size_t expected_length = strlen(expected);
    int listener;
    int connection;
    ssize_t got;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("socket");
    }
    say_line("socket-inet: socket fd=", listener, "\n");

    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("bind");
    }
    say("socket-inet: bind ok\n");

    if (listen(listener, 8) < 0) {
        return fail("listen");
    }
    say("socket-inet: listen ok\n");
    say_line("socket-inet: listening port=", PORT, "\n");

    // Wait for the hostfwd connection through the socket event source. A blocking `accept`
    // would fall into `NetBackend::wait`, which is still ENOSYS; `poll` is the platform path this
    // ticket extracts. Sessions that time out are stale host retries: close and wait for the
    // next connection so a retry that overlapped the listener start does not consume the test.
    for (int session = 0; session < 8; session++) {
        struct pollfd waiting;
        waiting.fd = listener;
        waiting.events = POLLIN;
        waiting.revents = 0;
        int ready = 0;
        for (int attempt = 0; attempt < 120; attempt++) {
            ready = poll(&waiting, 1, 1000);
            if (ready > 0) {
                break;
            }
            if (ready < 0 && errno != EINTR) {
                return fail("poll listener");
            }
        }
        if (ready <= 0 || (waiting.revents & POLLIN) == 0) {
            return fail("poll timeout");
        }
        say_line("socket-inet: poll revents=", (long)waiting.revents, "\n");

        peer_length = sizeof(peer);
        connection = accept(listener, (struct sockaddr *)&peer, &peer_length);
        if (connection < 0) {
            return fail("accept");
        }
        say_line("socket-inet: accept fd=", connection, "\n");
        say_peer(&peer);

        if (!wait_readable(connection, 5000)) {
            say("socket-inet: stale connection\n");
            (void)close(connection);
            continue;
        }
        got = read_exact(connection, payload, expected_length);
        if (got != (ssize_t)expected_length || memcmp(payload, expected, expected_length) != 0) {
            say("socket-inet: stale payload\n");
            (void)close(connection);
            continue;
        }
        say_line("socket-inet: echo received bytes=", (long)got, " payload=" PAYLOAD "\n");

        if (write_all(connection, payload, expected_length) != (ssize_t)expected_length) {
            return fail("write echo");
        }
        say("socket-inet: echo sent\n");

        if (!wait_readable(connection, 5000)) {
            say("socket-inet: no ack\n");
            (void)close(connection);
            continue;
        }
        got = read_exact(connection, ack, strlen("socket-inet-ack"));
        if (got != (ssize_t)strlen("socket-inet-ack")) {
            say("socket-inet: stale ack\n");
            (void)close(connection);
            continue;
        }
        ack[got] = '\0';
        if (strcmp(ack, "socket-inet-ack") != 0) {
            say("socket-inet: bad ack\n");
            (void)close(connection);
            continue;
        }
        say("socket-inet: host ack " "socket-inet-ack" "\n");
        if (close(connection) < 0) {
            return fail("close connection");
        }
        if (close(listener) < 0) {
            return fail("close listener");
        }
        say("socket-inet: ok\n");
        return 0;
    }
    return fail("no complete session");
}
