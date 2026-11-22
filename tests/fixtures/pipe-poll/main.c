/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

#include <poll.h>
#include <stddef.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

/* /init has no stdio fds; print through the kernel debug console, the same
 * DEBUG_WRITE (0x03) path the K195 pipe probe uses. */
static long debug_write(const char *bytes, size_t length) {
    register long number asm("rax") = 3;
    register const char *pointer asm("rdi") = bytes;
    register size_t len asm("rsi") = length;
    __asm__ volatile("syscall"
                     : "+r"(number)
                     : "r"(pointer), "r"(len)
                     : "rcx", "r11", "memory");
    return number;
}

static void say(const char *text) {
    (void)debug_write(text, strlen(text));
}

static int wait_event(int fd, short events, int timeout_ms) {
    struct pollfd pfd;
    int result;

    pfd.fd = fd;
    pfd.events = events;
    pfd.revents = 0;
    result = poll(&pfd, 1, timeout_ms);
    if (result <= 0) {
        return 0;
    }
    return pfd.revents;
}

static int fail(const char *what) {
    say("pipe-poll: fail ");
    say(what);
    say("\n");
    return 1;
}

int main(void) {
    int fds[2];
    int wake[2];
    char byte = 0;
    int revents;
    int status = 0;
    pid_t child;

    if (pipe(fds) != 0) {
        return fail("pipe");
    }
    revents = wait_event(fds[1], POLLOUT, 1000);
    if ((revents & POLLOUT) == 0) {
        return fail("pollout");
    }
    say("pipe-poll: pollout ok\n");

    if (write(fds[1], "x", 1) != 1) {
        return fail("write");
    }
    revents = wait_event(fds[0], POLLIN, 1000);
    if ((revents & POLLIN) == 0) {
        return fail("pollin");
    }
    say("pipe-poll: pollin ok\n");

    if (read(fds[0], &byte, 1) != 1 || byte != 'x') {
        return fail("read");
    }
    close(fds[1]);
    revents = wait_event(fds[0], POLLIN, 1000);
    if ((revents & (POLLHUP | POLLIN)) == 0) {
        return fail("hup");
    }
    say("pipe-poll: hup ok\n");
    close(fds[0]);

    if (pipe(wake) != 0) {
        return fail("wake-pipe");
    }
    child = fork();
    if (child == 0) {
        usleep(100000);
        (void)write(wake[1], "y", 1);
        _exit(0);
    }
    if (child < 0) {
        return fail("fork");
    }
    revents = wait_event(wake[0], POLLIN, 2000);
    if ((revents & POLLIN) == 0) {
        return fail("wake");
    }
    if (read(wake[0], &byte, 1) != 1 || byte != 'y') {
        return fail("wake-read");
    }
    (void)waitpid(child, &status, 0);
    say("pipe-poll: wake ok\n");
    close(wake[0]);
    close(wake[1]);

    say("pipe-poll: ok\n");
    return 0;
}
