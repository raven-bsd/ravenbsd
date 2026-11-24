// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K429 fixture: read a scheme socket into a buffer that was written before fork. The child's
// page is private copy-on-write, so the kernel capture must resolve COW for a writable read
// response; it used to reject the read-only PTE with EFAULT.

#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#define PORT 23082
#define READ_BYTES 32768
#define PAYLOAD "socket-inet-payload"
#define ACK "socket-read-efault-ack"

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

static int fail(const char *what) {
    char line[160];
    int length = snprintf(line, sizeof(line), "socket-read-efault: %s errno=%d\n", what, errno);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    say("socket-read-efault: fail\n");
    return 1;
}

static int wait_readable(int fd, int timeout_ms) {
    struct pollfd waiting;
    int ready;
    waiting.fd = fd;
    waiting.events = POLLIN;
    waiting.revents = 0;
    do {
        ready = poll(&waiting, 1, timeout_ms);
    } while (ready < 0 && errno == EINTR);
    return ready > 0 && (waiting.revents & POLLIN) != 0;
}

static ssize_t write_all(int fd, const char *data, size_t length) {
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
    int listener;
    int connection;
    char *buffer;
    pid_t child;
    int status = 0;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        return fail("socket");
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        return fail("bind");
    }
    if (listen(listener, 8) < 0) {
        return fail("listen");
    }
    say_value("socket-read-efault: listening port=", PORT, "\n");

    // This buffer is written before fork, so the child inherits a private COW page whose PTE is
    // read-only until a write fault. The read response must resolve that COW, not EFAULT.
    buffer = malloc(READ_BYTES);
    if (buffer == NULL) {
        return fail("malloc");
    }
    memset(buffer, 0x5a, READ_BYTES);

    for (int attempt = 0; attempt < 120; attempt++) {
        struct pollfd waiting;
        int ready;
        waiting.fd = listener;
        waiting.events = POLLIN;
        waiting.revents = 0;
        ready = poll(&waiting, 1, 1000);
        if (ready > 0 && (waiting.revents & POLLIN) != 0) {
            break;
        }
        if (ready < 0 && errno != EINTR) {
            return fail("poll listener");
        }
        if (attempt == 119) {
            return fail("poll timeout");
        }
    }

    connection = accept(listener, NULL, NULL);
    if (connection < 0) {
        return fail("accept");
    }
    say_value("socket-read-efault: accepted fd=", connection, "\n");

    child = fork();
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        ssize_t got;
        if (!wait_readable(connection, 5000)) {
            say("socket-read-efault: child poll timeout\n");
            _exit(4);
        }
        got = read(connection, buffer, READ_BYTES);
        if (got < 0) {
            say_value("socket-read-efault: child read errno=", errno, "\n");
            _exit(3);
        }
        if (got != (ssize_t)strlen(PAYLOAD) ||
            memcmp(buffer, PAYLOAD, strlen(PAYLOAD)) != 0) {
            say_value("socket-read-efault: child read bytes=", (long)got, "\n");
            _exit(5);
        }
        say_value("socket-read-efault: child read ok bytes=", (long)got, "\n");
        if (write_all(connection, ACK, strlen(ACK)) != (ssize_t)strlen(ACK)) {
            _exit(6);
        }
        _exit(0);
    }

    if (waitpid(child, &status, 0) != child) {
        return fail("waitpid");
    }
    say_value("socket-read-efault: child status=",
              WIFEXITED(status) ? WEXITSTATUS(status) : 255, "\n");
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        return 1;
    }
    (void)close(connection);
    (void)close(listener);
    free(buffer);
    say("socket-read-efault: ok\n");
    return 0;
}
