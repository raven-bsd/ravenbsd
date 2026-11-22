// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K222 static reader for the socketpair-exec fixture. It starts as a fresh image after execve,
// reads exactly N patterned bytes from the inherited descriptor, observes EOF, and proves the
// FD_CLOEXEC descriptor did not survive.

#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
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

static void say_errno(const char *what, int error) {
    char line[160];
    int length = snprintf(line, sizeof(line), "socketpair-exec: %s errno=%d\n", what, error);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
}

static int fail(const char *what) {
    say_errno(what, errno);
    say("socketpair-exec: fail\n");
    return 1;
}

static int parse(const char *text) {
    int value = 0;
    if (text == NULL || *text == '\0') {
        return -1;
    }
    while (*text != '\0') {
        if (*text < '0' || *text > '9') {
            return -1;
        }
        value = value * 10 + (*text - '0');
        text++;
    }
    return value;
}

int main(int argc, char **argv) {
    unsigned char buffer[754];
    int data_fd;
    int cloexec_fd;
    int shared_fd;
    int expected;
    size_t total = 0;
    ssize_t count;

    if (argc != 5) {
        return fail("reader argv");
    }
    data_fd = parse(argv[1]);
    expected = parse(argv[2]);
    cloexec_fd = parse(argv[3]);
    shared_fd = parse(argv[4]);
    if (data_fd < 0 || expected <= 0 || (size_t)expected > sizeof(buffer) ||
        cloexec_fd < 0 || shared_fd < 0) {
        return fail("reader args");
    }
    while (total < (size_t)expected) {
        count = read(data_fd, buffer + total, (size_t)expected - total);
        if (count <= 0) {
            return fail("reader read");
        }
        total += (size_t)count;
    }
    for (size_t index = 0; index < (size_t)expected; index++) {
        if (buffer[index] != (unsigned char)(index * 7u + 11u)) {
            return fail("reader pattern");
        }
    }
    say("socketpair-exec: exact bytes ok\n");

    count = read(data_fd, buffer, sizeof(buffer));
    if (count != 0) {
        return fail("reader eof");
    }
    say("socketpair-exec: eof ok\n");

    // data_fd and shared_fd were dup'd from one handle before exec. Closing the first must not
    // close the shared OFD; the second still sees the same stream and its EOF.
    if (close(data_fd) != 0) {
        return fail("reader close data");
    }
    errno = 0;
    count = read(shared_fd, buffer, sizeof(buffer));
    if (count != 0) {
        return fail("reader shared fd");
    }
    say("socketpair-exec: shared fd survives close\n");

    errno = 0;
    count = read(cloexec_fd, buffer, 1);
    if (count != -1 || errno != EBADF) {
        return fail("reader cloexec");
    }
    say("socketpair-exec: cloexec closed\n");
    say("socketpair-exec: reader ok\n");
    return 0;
}
