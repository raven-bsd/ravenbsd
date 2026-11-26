// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K222 target fixture: the rexec config-pipe shape. A socketpair is created before fork, the
// child duplicates its inherited end and execs a static reader, and the parent writes after the
// fork. The reader must see exactly N bytes and then EOF. A second descriptor carries
// FD_CLOEXEC and must not survive the exec.

#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
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

#define BYTE_COUNT 754

static void fill_pattern(unsigned char *buffer, size_t length) {
    for (size_t index = 0; index < length; index++) {
        buffer[index] = (unsigned char)(index * 7u + 11u);
    }
}

static int decimal(char *buffer, size_t size, long value) {
    int length = snprintf(buffer, size, "%ld", value);
    return length > 0 && (size_t)length < size ? 0 : -1;
}

int main(void) {
    int data[2];
    int close_on_exec[2];
    unsigned char payload[BYTE_COUNT];
    char data_fd[16];
    char count[16];
    char cloexec_fd[16];
    char shared_fd[16];
    char *reader_argv[6];
    char *reader_envp[1] = {NULL};
    pid_t child;
    ssize_t written;
    int status = 0;

    fill_pattern(payload, sizeof(payload));
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, data) != 0) {
        return fail("data socketpair");
    }
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, close_on_exec) != 0) {
        return fail("cloexec socketpair");
    }
    if (fcntl(close_on_exec[1], F_SETFD, FD_CLOEXEC) != 0) {
        return fail("cloexec fcntl");
    }

    child = fork();
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        int inherited;
        int duplicate;
        int shared;

        (void)close(data[0]);
        (void)close(close_on_exec[0]);
        inherited = data[1];
        duplicate = dup(inherited);
        shared = dup(inherited);
        if (duplicate < 0 || shared < 0) {
            return fail("child dup");
        }
        if (close(inherited) != 0) {
            return fail("child close original");
        }
        if (decimal(data_fd, sizeof(data_fd), duplicate) != 0 ||
            decimal(count, sizeof(count), BYTE_COUNT) != 0 ||
            decimal(cloexec_fd, sizeof(cloexec_fd), close_on_exec[1]) != 0 ||
            decimal(shared_fd, sizeof(shared_fd), shared) != 0) {
            return fail("child argv");
        }
        reader_argv[0] = (char *)"socketpair-exec-reader";
        reader_argv[1] = data_fd;
        reader_argv[2] = count;
        reader_argv[3] = cloexec_fd;
        reader_argv[4] = shared_fd;
        reader_argv[5] = NULL;
        execve("/bin/libc-child", reader_argv, reader_envp);
        return fail("child execve");
    }

    (void)close(data[1]);
    (void)close(close_on_exec[1]);
    written = write(data[0], payload, sizeof(payload));
    if (written != (ssize_t)sizeof(payload)) {
        return fail("parent write");
    }
    if (close(data[0]) != 0) {
        return fail("parent close");
    }
    (void)close(close_on_exec[0]);

    if (waitpid(child, &status, 0) != child) {
        return fail("parent waitpid");
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        char line[80];
        int length = snprintf(line, sizeof(line), "socketpair-exec: child status=%d\n", status);
        if (length > 0) {
            (void)raven_debug_write(line, (size_t)length);
        }
        say("socketpair-exec: fail\n");
        return 1;
    }
    say("socketpair-exec: parent ok\n");
    say("socketpair-exec: ok\n");
    return 0;
}
