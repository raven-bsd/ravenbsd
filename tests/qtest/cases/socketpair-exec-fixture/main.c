/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K195: reproduce the OpenSSH rexec fd shape over a static exec.
 *
 * AF_UNIX SOCK_STREAM pair, fork, the child writes 563 + 191 bytes, the parent duplicates
 * sv[1] to fd 4, closes sv[0]/sv[1], and execve()s a static reader. The reader proves the
 * exact bytes then EOF. This is the same fd shape as `config_s 10 -> 4` and 563/191 bytes. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

/* raven-abi Syscall::DEBUG_WRITE (0x03): debug_write(ptr, len) -> bytes written. */
static long raven_debug_write(const char *bytes, size_t len) {
    long result;
    __asm__ volatile("syscall"
                     : "=a"(result)
                     : "a"(3L), "D"(bytes), "S"(len)
                     : "rcx", "r11", "memory");
    return result;
}

static void say(const char *text) {
    (void)raven_debug_write(text, strlen(text));
}

static int fail(const char *what, int value) {
    char line[128];
    int length = snprintf(line, sizeof(line), "socketpair-exec: fail %s=%d errno=%d\n", what,
        value, errno);
    if (length > 0)
        (void)raven_debug_write(line, (size_t)length);
    return 1;
}

extern char **environ;

int main(void) {
    static char config[563];
    static char keys[191];
    int sv[2];

    memset(config, 'C', sizeof(config));
    memset(keys, 'K', sizeof(keys));

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0)
        return fail("socketpair", sv[0]);

    pid_t pid = fork();
    if (pid < 0)
        return fail("fork", (int)pid);

    if (pid == 0) {
        /* Writer: exactly the OpenSSH send_rexec_state child. */
        close(sv[1]);
        if (write(sv[0], config, sizeof(config)) != (ssize_t)sizeof(config))
            return fail("write-config", errno);
        if (write(sv[0], keys, sizeof(keys)) != (ssize_t)sizeof(keys))
            return fail("write-keys", errno);
        close(sv[0]);
        _exit(0);
    }

    /* Reader: the fd shape after config_s[1] == 10, then dup2 to REEXEC_CONFIG_PASS_FD=4. */
    (void)pid;
    close(sv[0]);
    if (dup2(sv[1], 4) != 4)
        return fail("dup2", errno);
    if (sv[1] != 4)
        close(sv[1]);

    char *const argv[] = { "/bin/socketpair-reader", NULL };
    execve(argv[0], argv, environ);
    return fail("execve", errno);
}
