// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// MPTCP2c guest fixture: prove that the two-tap fixture can detect a dead path (the negative
// control), then probe the MPTCP socket API the failover case needs.
//
// Negative mode: open a plain TCP connection to the MPTCP-capable host peer over the primary
// tap, send a deterministic 64 KiB stream, ask the host to unplug the primary mid-stream, and
// require that no host data arrives afterwards within NEGATIVE_BOUND_MS. That is the
// single-path control: a non-MPTCP connection dies when its only path is removed.
//
// Positive mode: create the MPTCP socket with IPPROTO_MPTCP and then the path manager. Both are
// the merged MPTCP2a/2b Rust API, but the current libc rejects the protocol and netd does not
// expose the path manager to a guest. Until that userland path lands the program prints the exact
// blocker and exits nonzero, so the failover case fails closed instead of reporting a fake pass.

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
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

static void say_value(const char *prefix, long value, const char *suffix) {
    char line[192];
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
    char line[192];
    int length = snprintf(line, sizeof(line), "mptcp: %s errno=%d\n", what, errno);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    say("mptcp: fail\n");
    return 1;
}

static long now_ms(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) < 0) {
        return 0;
    }
    return (long)now.tv_sec * 1000L + (long)(now.tv_nsec / 1000000L);
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

static int connect_peer(uint16_t port) {
    struct sockaddr_in peer;
    memset(&peer, 0, sizeof(peer));
    peer.sin_family = AF_INET;
    peer.sin_port = htons(port);
    peer.sin_addr.s_addr = inet_addr("10.77.1.2");

    for (int attempt = 0; attempt < 120; attempt++) {
        int fd = socket(AF_INET, SOCK_STREAM, 0);
        if (fd < 0) {
            return -1;
        }
        if (connect(fd, (const struct sockaddr *)&peer, sizeof(peer)) == 0) {
            return fd;
        }
        (void)close(fd);
        if (errno != ECONNREFUSED && errno != ENETUNREACH && errno != EHOSTUNREACH &&
            errno != ETIMEDOUT && errno != EINTR) {
            return -1;
        }
        struct pollfd waiting;
        waiting.fd = -1;
        waiting.events = 0;
        waiting.revents = 0;
        (void)poll(&waiting, 1, 500);
    }
    return -1;
}

#define NEGATIVE_BYTES 65536u
#define NEGATIVE_POLL_MS 4000L
#define NEGATIVE_BOUND_MS 6000L

static int run_negative(void) {
    unsigned char payload[4096];
    int fd = connect_peer(25021);
    if (fd < 0) {
        return fail("negative connect");
    }
    say("mptcp: negative connected\n");

    if (write_all(fd, "NEG-READY\n", 10) != 10) {
        return fail("negative ready write");
    }
    char go[3];
    if (read_exact(fd, go, sizeof(go)) != (ssize_t)sizeof(go) || memcmp(go, "GO\n", 3) != 0) {
        return fail("negative go read");
    }
    say("mptcp: negative go\n");

    for (size_t offset = 0; offset < NEGATIVE_BYTES; offset += sizeof(payload)) {
        size_t chunk = NEGATIVE_BYTES - offset;
        if (chunk > sizeof(payload)) {
            chunk = sizeof(payload);
        }
        for (size_t index = 0; index < chunk; index++) {
            payload[index] = (unsigned char)((offset + index) & 0xffu);
        }
        if (write_all(fd, payload, chunk) != (ssize_t)chunk) {
            return fail("negative stream write");
        }
    }
    say_value("mptcp: negative sent=", (long)NEGATIVE_BYTES, "\n");
    say("mptcp: negative unplug requested\n");

    // The host ticks every 200 ms for 3 s and unplugs the primary after 1 s. Read ticks for
    // 2 s to prove the path was alive, then require silence: a fixture that fails to unplug
    // keeps delivering ticks and trips unexpected-data below.
    long tick_started = now_ms();
    long tick_bytes = 0;
    while (now_ms() - tick_started < 2000) {
        struct pollfd ticker;
        ticker.fd = fd;
        ticker.events = POLLIN;
        ticker.revents = 0;
        int tick_ready;
        do {
            tick_ready = poll(&ticker, 1, 100);
        } while (tick_ready < 0 && errno == EINTR);
        if (tick_ready > 0 && (ticker.revents & POLLIN) != 0) {
            unsigned char buffer[64];
            ssize_t got = read(fd, buffer, sizeof(buffer));
            if (got <= 0) {
                break;
            }
            tick_bytes += (long)got;
        }
    }
    say_value("mptcp: negative tick_bytes=", tick_bytes, "\n");
    if (tick_bytes == 0) {
        say("mptcp: negative ok=0\n");
        return 1;
    }

    long started = now_ms();
    struct pollfd waiting;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    int ready;
    do {
        ready = poll(&waiting, 1, NEGATIVE_POLL_MS);
    } while (ready < 0 && errno == EINTR);
    long died_ms = now_ms() - started;
    if (ready > 0 && (waiting.revents & POLLIN) != 0) {
        unsigned char byte;
        ssize_t got = read(fd, &byte, 1);
        if (got == 0) {
            say("mptcp: negative closed\n");
        } else if (got > 0) {
            say("mptcp: negative unexpected-data\n");
            (void)close(fd);
            say("mptcp: negative ok=0\n");
            return 1;
        }
    }
    (void)close(fd);

    say_value("mptcp: negative died_ms=", died_ms, "\n");
    if (died_ms < 0 || died_ms > NEGATIVE_BOUND_MS) {
        say("mptcp: negative ok=0\n");
        return 1;
    }
    say("mptcp: negative ok=1\n");
    say_value("mptcp: negative bound_ms=", NEGATIVE_BOUND_MS, "\n");
    return 0;
}

static int probe_mptcp_socket(void) {
    int fd = socket(AF_INET, SOCK_STREAM, IPPROTO_MPTCP);
    if (fd < 0) {
        say_value("mptcp: probe IPPROTO_MPTCP errno=", (long)errno, "\n");
        say("mptcp: probe blocked\n");
        return -1;
    }
    say_value("mptcp: probe IPPROTO_MPTCP fd=", (long)fd, "\n");
    (void)close(fd);
    return 0;
}

static int run_positive(void) {
    int fd = socket(AF_INET, SOCK_STREAM, IPPROTO_MPTCP);
    if (fd < 0) {
        say_value("mptcp: socket(IPPROTO_MPTCP) errno=", (long)errno, "\n");
        say("mptcp: blocked mptcp2b-socket-api\n");
        return 1;
    }
    say_value("mptcp: socket fd=", (long)fd, "\n");
    (void)close(fd);
    // The path manager (add the backup path, mark it backup, query subflows) is part of the
    // merged MPTCP2a/2b core but has no guest socket surface yet. Without it the fixture cannot
    // open the second path, so it fails closed rather than pretending a one-path connection is
    // the failover acceptance.
    say("mptcp: blocked mptcp2b-path-manager\n");
    return 1;
}

int main(void) {
#if defined(MPTCP_FIXTURE_MODE_POSITIVE)
    const char *mode = "positive";
#else
    const char *mode = "negative";
#endif
    char mode_line[64];
    int mode_length = snprintf(mode_line, sizeof(mode_line), "mptcp: fixture mode=%s\n", mode);
    if (mode_length > 0) {
        size_t mode_size = (size_t)mode_length;
        if (mode_size >= sizeof(mode_line)) {
            mode_size = sizeof(mode_line) - 1;
        }
        (void)raven_debug_write(mode_line, mode_size);
    }

    if (strcmp(mode, "positive") == 0) {
        return run_positive();
    }

    int result = run_negative();
    if (result != 0) {
        return result;
    }
    (void)probe_mptcp_socket();
    say("mptcp: negative done\n");
    return 0;
}
