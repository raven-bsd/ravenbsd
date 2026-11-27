/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* S173b PG-26 pointer assertion.
 *
 * The mapped control is a valid `struct pollfd[1]` with fd=-1 and events=0; with no
 * registration the wait is empty, so `ppoll` returns 0. The boundary is the exact
 * in-window address 0x1000 (`USER_MIN`), which is not mapped: `ppoll((struct pollfd
 * *)0x1000, 1, NULL, NULL)` must return -1/EFAULT. It runs in a forked child so the
 * parent can prove the child exited normally and was never killed by signal 11.
 *
 * The guest is /init and has no stdio fds; progress goes through the kernel debug
 * console (DEBUG_WRITE, 0x03), the same path the K372 pipe-poll fixture uses. */

#include <errno.h>
#include <poll.h>
#include <stddef.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

#define IN_WINDOW_UNMAPPED ((struct pollfd *)0x1000)

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

static void say_uint(unsigned long value) {
    char digits[20];
    size_t used = 0;

    do {
        digits[used++] = (char)('0' + (value % 10));
        value /= 10;
    } while (value != 0);
    while (used != 0) {
        --used;
        (void)debug_write(&digits[used], 1);
    }
}

static int fail(const char *what) {
    say("pg26-pointer: fail ");
    say(what);
    say("\n");
    return 1;
}

int main(void) {
    struct pollfd control;
    int result;
    int status = 0;
    pid_t child;

    control.fd = -1;
    control.events = 0;
    control.revents = 0;
    errno = 0;
    result = ppoll(&control, 1, NULL, NULL);
    if (result != 0 || errno != 0) {
        say("pg26-pointer: fail mapped result=");
        say_uint((unsigned long)(long)result);
        say(" errno=");
        say_uint((unsigned long)errno);
        say("\n");
        return 1;
    }
    say("pg26-pointer: mapped ok\n");

    child = fork();
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        errno = 0;
        result = ppoll(IN_WINDOW_UNMAPPED, 1, NULL, NULL);
        if (result == -1 && errno == EFAULT) {
            _exit(0);
        }
        _exit(result == -1 ? 2 : 3);
    }
    if (waitpid(child, &status, 0) < 0) {
        return fail("waitpid");
    }
    if (!WIFEXITED(status)) {
        say("pg26-pointer: fail child-signal=");
        say_uint((unsigned long)WTERMSIG(status));
        say("\n");
        return 1;
    }
    if (WEXITSTATUS(status) != 0) {
        say("pg26-pointer: fail child-exit=");
        say_uint((unsigned long)WEXITSTATUS(status));
        say("\n");
        return 1;
    }
    say("pg26-pointer: in-window EFAULT\n");
    return 0;
}
