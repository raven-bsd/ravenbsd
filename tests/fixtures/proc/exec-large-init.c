/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K220: small libc init that proves a failed exec leaves the image running, then execs the
 * >8 MiB static helper and keeps the pid/argv/environment. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("exec-large: fail %s=%d errno=%d\n", what, value, errno);
    fflush(stdout);
    return 1;
}

int main(void) {
    char pid_text[32];
    snprintf(pid_text, sizeof(pid_text), "%d", (int)getpid());
    printf("exec-large: start pid=%s\n", pid_text);
    fflush(stdout);

    int fd = open("/tmp/not-an-elf", O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0)
        return fail("open", fd);
    if (write(fd, "not an elf", 10) != 10)
        return fail("write", -1);
    close(fd);

    char *bad_argv[] = {(char *)"/tmp/not-an-elf", NULL};
    char *bad_envp[] = {NULL};
    execve("/tmp/not-an-elf", bad_argv, bad_envp);
    if (errno != ENOEXEC)
        return fail("bad-elf errno", errno);
    printf("exec-large: bad-elf refused errno=%d\n", errno);
    fflush(stdout);

    char pid_env[64];
    snprintf(pid_env, sizeof(pid_env), "PROC_EXEC_PID=%s", pid_text);
    char *argv[] = {(char *)"/bin/libc-child", (char *)"alpha", (char *)"beta", NULL};
    char *envp[] = {pid_env, (char *)"PROC_EXEC_TOKEN=K220", NULL};
    execve("/bin/libc-child", argv, envp);
    return fail("exec", errno);
}
