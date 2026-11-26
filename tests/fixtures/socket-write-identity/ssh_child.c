// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K431 S5 identity child. It is exec'd with the accepted TCP socket on fd 7 and the same socket
// duplicated onto fd 1 and fd 2. It asserts all three descriptors name the same netd file id
// (`st_ino`), then writes a distinct sentinel on each descriptor. The host peer must receive the
// three sentinels in order; fd 1/2 must not fall back to the console.

#include <errno.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
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

static int read_exact(int fd, char *data, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t got = read(fd, data + done, length - done);
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (got == 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

static int fd_ino(int fd, unsigned long long *ino) {
    struct stat record;
    if (fstat(fd, &record) < 0) {
        say_value("socket-write-identity-child: fstat errno=", errno, "");
        say_value(" fd=", fd, "\n");
        return -1;
    }
    if (!S_ISSOCK(record.st_mode)) {
        say_value("socket-write-identity-child: not a socket fd=", fd, "\n");
        return -1;
    }
    *ino = (unsigned long long)record.st_ino;
    return 0;
}

int main(int argc, char **argv) {
    unsigned long long ino7 = 0;
    unsigned long long ino0 = 0;
    unsigned long long ino1 = 0;
    unsigned long long ino2 = 0;
    int descriptor;
    struct pollfd waiting;

    if (argc < 2) {
        return 2;
    }
    descriptor = atoi(argv[1]);
    if (descriptor < 0) {
        return 3;
    }
    if (fd_ino(descriptor, &ino7) != 0 || fd_ino(0, &ino0) != 0 ||
        fd_ino(1, &ino1) != 0 || fd_ino(2, &ino2) != 0) {
        return 4;
    }
    if (ino7 == 0 || ino0 != ino7 || ino1 != ino7 || ino2 != ino7) {
        say_value("socket-write-identity-child: ident mismatch fd0=", (long)ino0, "");
        say_value(" fd1=", (long)ino1, "");
        say_value(" fd2=", (long)ino2, "");
        say_value(" fd7=", (long)ino7, "\n");
        return 5;
    }
    say_value("socket-write-identity-child: ident ino=", (long)ino7, "");
    say_value(" fd7=", descriptor, " fd0=0 fd1=1 fd2=2\n");

    waiting.fd = descriptor;
    waiting.events = POLLOUT;
    waiting.revents = 0;
    {
        int ready;
        do {
            ready = poll(&waiting, 1, POLL_TIMEOUT_MS);
        } while (ready < 0 && errno == EINTR);
        if (ready == 0) {
            say("socket-write-identity-child: pollout timeout\n");
            return 6;
        }
        if (ready < 0 || (waiting.revents & POLLOUT) == 0) {
            say("socket-write-identity-child: pollout failed\n");
            return 7;
        }
    }
    say("socket-write-identity-child: pollout ready\n");

    if (write_all(descriptor, "ID7-CHAN", 8) != 0) {
        say_value("socket-write-identity-child: fd7 write errno=", errno, "\n");
        return 8;
    }
    say("socket-write-identity-child: chan sent\n");

    {
        char ping[4];
        if (read_exact(descriptor, ping, sizeof(ping)) != 0 ||
            memcmp(ping, "PING", sizeof(ping)) != 0) {
            say("socket-write-identity-child: ping missing\n");
            return 11;
        }
    }
    if (write_all(descriptor, "ID7-PONG", 8) != 0) {
        say_value("socket-write-identity-child: pong write errno=", errno, "\n");
        return 12;
    }
    say("socket-write-identity-child: pong sent\n");
    return 0;
}
