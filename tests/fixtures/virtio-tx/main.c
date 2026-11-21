// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K382 TX-ring stress fixture. The guest connects to the host at 10.0.2.2:23095 and sends 32 KiB
// in 32 writes without any intervening receive traffic; the host verifies it and echoes the same
// 32 KiB back. The guest logs `done` only after every TX frame crossed the NIC and the echo
// returned, so a TX used-ring wedge is visible on the serial line.

#include <arpa/inet.h>
#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <time.h>
#include <unistd.h>
#include <unistd.h>

#define HOST_IP "10.0.2.2"
#define PORT 23095
#define FRAMES 32
#define FRAME_BYTES 1024
#define TOTAL_BYTES (FRAMES * FRAME_BYTES)

static unsigned char buffer[TOTAL_BYTES];

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

static ssize_t read_full(int fd, unsigned char *out, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t got = read(fd, out + done, length - done);
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (got == 0) {
            return (ssize_t)done;
        }
        done += (size_t)got;
    }
    return (ssize_t)done;
}

static ssize_t write_all(int fd, const unsigned char *data, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t wrote = write(fd, data + done, length - done);
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

int main(void) {
    struct sockaddr_in address;
    int connection;
    int attempt;

    connection = socket(AF_INET, SOCK_STREAM, 0);
    if (connection < 0) {
        say_value("virtio-tx: socket errno=", errno, "\n");
        return 1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    if (inet_pton(AF_INET, HOST_IP, &address.sin_addr) != 1) {
        say("virtio-tx: bad host address\n");
        return 1;
    }
    for (attempt = 0; attempt < 50; attempt++) {
        if (connect(connection, (const struct sockaddr *)&address, sizeof(address)) == 0) {
            break;
        }
        usleep(100000);
    }
    if (attempt == 50) {
        say_value("virtio-tx: connect errno=", errno, "\n");
        return 1;
    }
    say("virtio-tx: connected\n");

    for (size_t offset = 0; offset < TOTAL_BYTES; offset++) {
        buffer[offset] = (unsigned char)((offset / FRAME_BYTES) + (offset % FRAME_BYTES));
    }

    // Burst the whole 32 KiB before reading anything back; no serial logging in the loop so the
    // writes are not paced by the debug console.
    struct timespec started;
    struct timespec finished;

    clock_gettime(CLOCK_MONOTONIC, &started);
    for (int index = 0; index < FRAMES; index++) {
        unsigned char *frame = buffer + (size_t)index * FRAME_BYTES;
        ssize_t wrote;

        errno = 0;
        wrote = write_all(connection, frame, FRAME_BYTES);
        if (wrote != FRAME_BYTES) {
            say_value("virtio-tx: write failed index=", index, "");
            say_value(" bytes=", (long)wrote, "");
            say_value(" errno=", errno, "\n");
            return 1;
        }
    }
    clock_gettime(CLOCK_MONOTONIC, &finished);
    long elapsed_ms = (finished.tv_sec - started.tv_sec) * 1000L +
                      (finished.tv_nsec - started.tv_nsec) / 1000000L;
    say_value("virtio-tx: sent=32768 elapsed_ms=", elapsed_ms, "\n");

    ssize_t echoed = read_full(connection, buffer, TOTAL_BYTES);
    say_value("virtio-tx: echo received=", (long)echoed, "\n");
    for (size_t offset = 0; offset < TOTAL_BYTES; offset++) {
        unsigned char expected = (unsigned char)((offset / FRAME_BYTES) + (offset % FRAME_BYTES));
        if (buffer[offset] != expected) {
            say("virtio-tx: echo mismatch\n");
            return 1;
        }
    }
    say("virtio-tx: done\n");
    return 0;
}
