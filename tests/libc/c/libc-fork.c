/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K196: POSIX fork. The child returns 0, the parent the pid; the child's write to a COW
 * variable stays private, the inherited descriptor writes reach the same file, and waitpid
 * reports the child's exit status. */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

#define MARK "/tmp/libc-fork-fd"

static int fail(const char *what, int value) {
    printf("libc-fork: fail %s=%d errno=%d\n", what, value, errno);
    return 1;
}

int main(void) {
    printf("libc-fork: start\n");

    int fd = open(MARK, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0)
        return fail("open", fd);

    int value = 7;
    pid_t parent = getpid();
    pid_t pid = fork();
    if (pid < 0)
        return fail("fork", (int)pid);

    if (pid == 0) {
        if (getpid() == parent)
            _exit(2);
        value = 99;
        if (write(fd, "child\n", 6) != 6)
            _exit(4);
        close(fd);
        printf("libc-fork: child value=%d\n", value);
        fflush(stdout);
        _exit(7);
    }

    printf("libc-fork: parent pid=%d\n", (int)pid);
    close(fd);

    int status = 0;
    pid_t waited = waitpid(pid, &status, 0);
    if (waited != pid)
        return fail("waitpid", (int)waited);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 7)
        return fail("status", status);
    printf("libc-fork: wait status=7\n");

    if (value != 7)
        return fail("cow", value);
    printf("libc-fork: cow ok\n");

    int read_fd = open(MARK, O_RDONLY);
    if (read_fd < 0)
        return fail("reopen", read_fd);
    char buffer[16];
    memset(buffer, 0, sizeof(buffer));
    ssize_t count = read(read_fd, buffer, sizeof(buffer) - 1);
    close(read_fd);
    if (count != 6 || memcmp(buffer, "child\n", 6) != 0)
        return fail("inherited fd", (int)count);
    printf("libc-fork: fd ok\n");

    printf("libc-fork: ok\n");
    return 0;
}
