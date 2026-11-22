// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K435 K4 helper: the fork+exec child sees the same socket by fd number, waits for PING,
// polls POLLOUT and answers PONG on the inherited open file description.

#include <errno.h>
#include <poll.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

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

static int read_exact(int fd, void *buffer, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t got = read(fd, (char *)buffer + done, length - done);
        if (got <= 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

static int write_exact(int fd, const void *buffer, size_t length) {
    size_t done = 0;

    while (done < length) {
        ssize_t wrote = write(fd, (const char *)buffer + done, length - done);
        if (wrote <= 0) {
            return -1;
        }
        done += (size_t)wrote;
    }
    return 0;
}

int main(int argc, char **argv) {
    int fd;
    struct pollfd pfd;
    char buffer[4];
    int ready;
    const char *mode = argc > 2 ? argv[2] : "";

    if (argc < 2) {
        return 2;
    }
    fd = atoi(argv[1]);
    if (fd < 0) {
        return 3;
    }
    {
        char line[80];
        (void)snprintf(line, sizeof(line), "CONTRACT K4 CHILD fd=%d\n", fd);
        say(line);
    }
    if (strcmp(mode, "eof") == 0) {
        pfd.fd = fd;
        pfd.events = POLLIN | POLLHUP;
        pfd.revents = 0;
        do {
            ready = poll(&pfd, 1, 5000);
        } while (ready < 0 && errno == EINTR);
        if (ready <= 0) {
            say("CONTRACT K8 CHILD poll timeout\n");
            _exit(8);
        }
        {
            char byte = 0;
            ssize_t got = read(fd, &byte, 1);
            char line[128];
            (void)snprintf(line, sizeof(line),
                           "CONTRACT K8 CHILD eof got=%d revents=0x%x errno=%d\n",
                           (int)got, (unsigned)pfd.revents, errno);
            say(line);
            if (got != 0) {
                _exit(9);
            }
        }
        _exit(0);
    }
    pfd.fd = fd;
    pfd.events = POLLIN;
    pfd.revents = 0;
    do {
        ready = poll(&pfd, 1, 5000);
    } while (ready < 0 && errno == EINTR);
    {
        char line[96];
        (void)snprintf(line, sizeof(line), "CONTRACT K4 CHILD poll ready=%d revents=0x%x errno=%d\n",
                       ready, (unsigned)pfd.revents, errno);
        say(line);
    }
    if (ready <= 0 || (pfd.revents & POLLIN) == 0) {
        _exit(4);
    }
    if (read_exact(fd, buffer, sizeof(buffer)) != 0 || memcmp(buffer, "PING", 4) != 0) {
        say("CONTRACT K4 CHILD read fail\n");
        _exit(5);
    }
    say("CONTRACT K4 CHILD read PING\n");
    pfd.events = POLLOUT;
    pfd.revents = 0;
    do {
        ready = poll(&pfd, 1, 5000);
    } while (ready < 0 && errno == EINTR);
    if (ready <= 0 || (pfd.revents & POLLOUT) == 0) {
        _exit(6);
    }
    if (write_exact(fd, "PONG", 4) != 0) {
        say("CONTRACT K4 CHILD write fail\n");
        _exit(7);
    }
    say("CONTRACT K4 CHILD wrote PONG\n");
    _exit(0);
}
