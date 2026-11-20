// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Target entry for the ssh-version qtest. It calls the upstream sshd main with "-V" and routes
// the version line to the RavenBSD debug console. It does not synthesize a version: the string
// comes from the unmodified upstream sshd/version.h. The small exit delay lets svcd observe the
// one-shot service before it exits.

#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

int sshd_main(int argc, char **argv);

// The minimal qtest initfs has no /dev mount before svcd starts the service. sshd opens
// /dev/null early; hand it a valid read/write fd (a private socketpair, then the staged binary
// as a read-only fallback) and leave every other path to libc's openat. This is a test harness
// fd, not resolver or interface data.
int open(const char *path, int flags, ...) {
    if (path != NULL && strcmp(path, "/dev/null") == 0) {
        int fds[2];
        if (socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0) {
            (void)close(fds[1]);
            return fds[0];
        }
        int socket_error = errno;
        int fd = openat(AT_FDCWD, "/bin/ssh-version", O_RDONLY);
        int open_error = errno;
        fprintf(stderr, "ssh-version /dev/null: socketpair errno=%d openat errno=%d\n",
                socket_error, open_error);
        return fd;
    }
    mode_t mode = 0;
    if ((flags & O_CREAT) != 0) {
        va_list arguments;
        va_start(arguments, flags);
        mode = (mode_t)va_arg(arguments, int);
        va_end(arguments);
    }
    return openat(AT_FDCWD, path, flags, mode);
}

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

static int debug_format(const char *format, va_list arguments) {
    char buffer[4096];
    int length = vsnprintf(buffer, sizeof(buffer), format, arguments);
    if (length > 0) {
        size_t size = (size_t)length;
        if (size >= sizeof(buffer)) {
            size = sizeof(buffer) - 1;
        }
        (void)raven_debug_write(buffer, size);
    }
    return length;
}

int vfprintf(FILE *stream, const char *format, va_list arguments) {
    (void)stream;
    return debug_format(format, arguments);
}

int fprintf(FILE *stream, const char *format, ...) {
    (void)stream;
    va_list arguments;
    va_start(arguments, format);
    int length = debug_format(format, arguments);
    va_end(arguments);
    return length;
}

void exit(int status) {
    struct timespec delay = {0, 200000000};
    (void)nanosleep(&delay, NULL);
    _exit(status);
}

int main(void) {
    char program[] = "sshd";
    char *argv[] = {program, "-V", NULL};
    return sshd_main(2, argv);
}
