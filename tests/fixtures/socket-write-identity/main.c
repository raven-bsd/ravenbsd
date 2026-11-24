// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K431 fixture: a pipe whose read end is switched to O_NONBLOCK with fcntl() must not sleep in
// the kernel. OpenSSH creates its session pipes with pipe(), then marks them non-blocking; the
// Raven pipe end captured the flag at creation only, so the server's first non-blocking read
// slept forever and the channel confirmation was never flushed. The watchdog below prints
// `FAIL read blocked` if the read does not return, which the qtest forbids.

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
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

static int fail(const char *what) {
    char line[192];
    int length = snprintf(line, sizeof(line),
                          "socket-write-identity: %s errno=%d\n", what, errno);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(line)) {
            size = sizeof(line) - 1;
        }
        (void)raven_debug_write(line, size);
    }
    say("socket-write-identity: fail\n");
    return 1;
}

static int set_nonblock(int fd) {
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0) {
        return -1;
    }
    return fcntl(fd, F_SETFL, flags | O_NONBLOCK);
}

// Watchdog: if the non-blocking read does not return within two seconds, report it. The parent
// signals `done` on success so the watchdog exits silently.
static void watchdog(int done_fd) {
    struct pollfd waiting = {done_fd, POLLIN, 0};
    if (poll(&waiting, 1, 20000) == 0) {
        say("socket-write-identity: FAIL read blocked\n");
        _exit(9);
    }
    _exit(0);
}

int main(void) {
    int pipefd[2];
    int done[2];
    pid_t dog;
    char byte = 0;
    ssize_t result;

    if (pipe(pipefd) < 0 || pipe(done) < 0) {
        return fail("pipe");
    }
    dog = fork();
    if (dog < 0) {
        return fail("watchdog fork");
    }
    if (dog == 0) {
        (void)close(done[1]);
        watchdog(done[0]);
    }
    (void)close(done[0]);
    if (set_nonblock(pipefd[0]) < 0) {
        return fail("fcntl nonblock");
    }

    result = read(pipefd[0], &byte, 1);
    if (result != -1 || errno != EAGAIN) {
        return fail("nonblock empty read");
    }
    say("socket-write-identity: nonblock empty read EAGAIN ok\n");

    // A non-blocking write end must not sleep when the ring is full.
    if (set_nonblock(pipefd[1]) < 0) {
        return fail("fcntl write nonblock");
    }
    {
        char block[4096];
        memset(block, 'p', sizeof(block));
        for (int index = 0; index < 32; index++) {
            (void)write(pipefd[1], block, sizeof(block));
        }
    }
    result = write(pipefd[1], &byte, 1);
    if (result != -1 || errno != EAGAIN) {
        return fail("nonblock full write");
    }
    say("socket-write-identity: nonblock full write EAGAIN ok\n");

    // Drain and prove a blocking reader still waits for a delayed writer.
    {
        char drain[4096];
        while (read(pipefd[0], drain, sizeof(drain)) > 0) {
        }
    }
    {
        int blocking[2];
        pid_t writer;
        char got[4];
        if (pipe(blocking) < 0) {
            return fail("blocking pipe");
        }
        writer = fork();
        if (writer < 0) {
            return fail("writer fork");
        }
        if (writer == 0) {
            (void)close(blocking[0]);
            (void)poll(NULL, 0, 50);
            (void)write(blocking[1], "pipe", 4);
            _exit(0);
        }
        (void)close(blocking[1]);
        if (read(blocking[0], got, sizeof(got)) != (ssize_t)sizeof(got) ||
            memcmp(got, "pipe", sizeof(got)) != 0) {
            return fail("blocking read");
        }
        (void)waitpid(writer, NULL, 0);
        (void)close(blocking[0]);
    }
    say("socket-write-identity: blocking round-trip ok\n");

    if (write(done[1], &byte, 1) != 1) {
        return fail("watchdog signal");
    }
    if (waitpid(dog, NULL, 0) != dog) {
        return fail("watchdog wait");
    }
    (void)close(pipefd[0]);
    (void)close(pipefd[1]);
    (void)close(done[1]);
    say("socket-write-identity: ok\n");
    return 0;
}
