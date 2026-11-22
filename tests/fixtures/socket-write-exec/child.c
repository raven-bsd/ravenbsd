// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K408/K409 fork+exec child: the inherited accepted/connected socket must report POLLOUT after
// `exec` and then write the "EXEC" marker to it. K409's post-auth SSH failure was a poll that
// never fired before the queued write.

#include <errno.h>
#include <poll.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#define POLL_TIMEOUT_MS 5000

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
    size_t length = 0;

    while (text[length] != '\0') {
        length++;
    }
    (void)raven_debug_write(text, length);
}

static int write_all(int fd, const char *data, size_t length) {
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
    return 0;
}

int main(int argc, char **argv) {
    int descriptor;

    if (argc < 2) {
        return 2;
    }
    descriptor = atoi(argv[1]);
    if (descriptor < 0) {
        return 3;
    }

    // The post-auth SSH shape: poll the inherited socket for writability before queueing the
    // pending output. A missing readiness event leaves this asleep until the timeout.
    {
        struct pollfd waiting;
        int ready;

        waiting.fd = descriptor;
        waiting.events = POLLOUT;
        waiting.revents = 0;
        do {
            ready = poll(&waiting, 1, POLL_TIMEOUT_MS);
        } while (ready < 0 && errno == EINTR);
        if (ready == 0) {
            say("socket-write-exec-child: pollout timeout\n");
            return 5;
        }
        if (ready < 0 || (waiting.revents & POLLOUT) == 0) {
            say("socket-write-exec-child: pollout failed\n");
            return 6;
        }
        say("socket-write-exec-child: pollout ready\n");
    }

    if (write_all(descriptor, "EXEC", 4) != 0) {
        char line[96];
        int length = snprintf(line, sizeof(line),
                              "socket-write-exec-child: exec write errno=%d\n", errno);
        if (length > 0) {
            (void)raven_debug_write(line, (size_t)length);
        }
        return 4;
    }
    return 0;
}
