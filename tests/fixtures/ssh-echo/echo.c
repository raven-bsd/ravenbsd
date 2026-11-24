// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K434 S0 guest echo fixture. The guest owns a TCP listener on the K194 tap; the host connects,
// sends one exact payload, and must receive the same bytes back. The program is test tooling only
// and is staged into a throw-away `server` initfs by build.sh, never into the release image.

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#define PORT 23097
#define PAYLOAD_MAX 64

// raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written.
#if defined(__aarch64__)
static long raven_debug_write(const char *bytes, size_t len) {
    register long result asm("x0") = 0;
    register long number asm("x8") = 3;
    register const char *pointer asm("x0") = bytes;
    register size_t length asm("x1") = len;
    __asm__ volatile("svc #0"
                     : "+r"(pointer)
                     : "r"(number), "r"(length)
                     : "memory");
    result = (long)(size_t)pointer;
    return result;
}
#else
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
#endif

static void say(const char *text) {
    size_t length = 0;

    while (text[length] != '\0') {
        length++;
    }
    (void)raven_debug_write(text, length);
}

static void say_value(const char *prefix, long value, const char *suffix) {
    char line[128];
    int length = snprintf(line, sizeof(line), "%s%ld%s", prefix, value, suffix);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int write_all(int fd, const unsigned char *data, size_t length) {
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

int main(void) {
    struct sockaddr_in address;
    unsigned char payload[PAYLOAD_MAX];
    size_t length = 0;
    int listener;

    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        say_value("ssh-echo: socket errno=", errno, "\n");
        return 1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        say_value("ssh-echo: bind errno=", errno, "\n");
        return 1;
    }
    if (listen(listener, 8) < 0) {
        say_value("ssh-echo: listen errno=", errno, "\n");
        return 1;
    }
    say("ssh-echo: listening port=23097\n");

    for (;;) {
        int connection = accept(listener, NULL, NULL);
        ssize_t got;

        if (connection < 0) {
            if (errno == EINTR) {
                continue;
            }
            say_value("ssh-echo: accept errno=", errno, "\n");
            return 1;
        }
        while (length < sizeof(payload)) {
            got = read(connection, payload + length, sizeof(payload) - length);
            if (got < 0) {
                if (errno == EINTR) {
                    continue;
                }
                say_value("ssh-echo: read errno=", errno, "\n");
                break;
            }
            if (got == 0) {
                break;
            }
            length += (size_t)got;
            if (payload[length - 1] == '\n') {
                break;
            }
        }
        if (length > 0 && write_all(connection, payload, length) != 0) {
            say_value("ssh-echo: write errno=", errno, "\n");
        } else {
            say_value("ssh-echo: echoed ", (long)length, "\n");
        }
        length = 0;
        close(connection);
    }
}
