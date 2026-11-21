// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K289 target fixture: one bulk TCP sender for the CUBIC/RACK wire scenarios.
// The raw peer controls the ACK/window pattern; the guest sends from this
// buffer and reports exact byte counts on the serial line.

#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <poll.h>
#include <unistd.h>

#define PORT 23091
#define TOTAL (256u * 1024u)
#define CHUNK 1460u

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

int main(void) {
    struct sockaddr_in address;
    char buffer[CHUNK];
    int listener;
    int connection;
    size_t sent = 0;

    memset(buffer, 'A', sizeof(buffer));
    listener = socket(AF_INET, SOCK_STREAM, 0);
    if (listener < 0) {
        say_value("cc: socket errno=", errno, "\n");
        return 1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(PORT);
    address.sin_addr.s_addr = htonl(0);
    if (bind(listener, (const struct sockaddr *)&address, sizeof(address)) < 0) {
        say_value("cc: bind errno=", errno, "\n");
        return 1;
    }
    if (listen(listener, 8) < 0) {
        say_value("cc: listen errno=", errno, "\n");
        return 1;
    }
    say("cc: listening port=23091\n");
    connection = accept(listener, NULL, NULL);
    if (connection < 0) {
        say_value("cc: accept errno=", errno, "\n");
        return 1;
    }
    say("cc: accepted\n");
    while (sent < TOTAL) {
        size_t left = TOTAL - sent;
        size_t want = left < sizeof(buffer) ? left : sizeof(buffer);
        ssize_t wrote = write(connection, buffer, want);
        if (wrote < 0) {
            if (errno == EINTR) {
                continue;
            }
            say_value("cc: write errno=", errno, "\n");
            break;
        }
        if (wrote == 0) {
            break;
        }
        sent += (size_t)wrote;
    }
    say_value("cc: sent=", (long)sent, "\n");
    (void)close(connection);
    say("cc: closed\n");
    return sent == TOTAL ? 0 : 1;
}
