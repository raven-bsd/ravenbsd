// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K195 platform probe: the libc `pipe(2)` primitive. Upstream OpenSSH session.c
// (`do_exec_no_pty`) allocates the command session's stdio with three `pipe(2)`
// calls, so a host `ssh guest uname -a` cannot work until libc implements it.
//
// The probe prints its real result through the kernel debug console (the same
// `DEBUG_WRITE` syscall the K200 version smoke uses): a service starts with no
// stdio fds, so stdout is not available. It writes no files and is not part of
// the release image.

#include <errno.h>
#include <stddef.h>
#include <unistd.h>

// raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written.
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

static void say(const char *text, size_t length) {
    (void)debug_write(text, length);
}

int main(void) {
    int fds[2];
    if (pipe(fds) != 0) {
        int error = errno;
        if (error == ENOSYS) {
            static const char line[] = "pipe-probe: ENOSYS\n";
            say(line, sizeof(line) - 1);
        } else {
            static const char line[] = "pipe-probe: ERROR\n";
            say(line, sizeof(line) - 1);
        }
        return 1;
    }
    {
        static const char line[] = "pipe-probe: pipe fd\n";
        say(line, sizeof(line) - 1);
    }
    if (write(fds[1], "x", 1) != 1) {
        static const char line[] = "pipe-probe: WRITE FAILED\n";
        say(line, sizeof(line) - 1);
        return 1;
    }
    {
        char byte = 0;
        if (read(fds[0], &byte, 1) != 1 || byte != 'x') {
            static const char line[] = "pipe-probe: READ FAILED\n";
            say(line, sizeof(line) - 1);
            return 1;
        }
    }
    (void)close(fds[0]);
    (void)close(fds[1]);
    {
        static const char line[] = "pipe-probe: ok\n";
        say(line, sizeof(line) - 1);
    }
    return 0;
}
