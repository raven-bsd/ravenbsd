/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * K493: tcgetpgrp/tcsetpgrp over the pty server's foreground group.
 *
 * The controller opens a pty master, sets its own process group as the foreground group, forks
 * and execs /bin/libc-test in child mode (the descriptor survives), then checks the non-tty and
 * cross-session errors. The child only reads tcgetpgrp and exits.
 */

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

#define SELF "/bin/libc-test"

static int child_mode(int argc, char **argv) {
    int fd = argc > 2 ? atoi(argv[2]) : -1;
    int expected = argc > 3 ? atoi(argv[3]) : -1;
    errno = 0;
    int actual = tcgetpgrp(fd);
    int ok = actual == expected && errno == 0;
    printf("tty-pgrp: child actual=%d expected=%d ok=%d\n", actual, expected, ok);
    fflush(stdout);
    return ok ? 0 : 1;
}

static int wait_child(pid_t pid) {
    int status = 0;
    if (waitpid(pid, &status, 0) != pid)
        return 0;
    return WIFEXITED(status) && WEXITSTATUS(status) == 0;
}

static int fail(const char *what, int fd_errno) {
    printf("tty-pgrp: FAIL %s errno=%d\n", what, fd_errno);
    fflush(stdout);
    return 0;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "child") == 0)
        return child_mode(argc, argv);

    printf("tty-pgrp: start\n");
    fflush(stdout);

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0 || grantpt(master) != 0 || unlockpt(master) != 0)
        return !fail("open-pty", errno);
    int initial = tcgetpgrp(master);
    if (initial < 0)
        return !fail("get-initial", errno);
    printf("tty-pgrp: get-initial pgrp=%d\n", initial);
    fflush(stdout);

    if (setpgid(0, 0) != 0)
        return !fail("setpgid", errno);
    int own = getpgrp();
    if (tcsetpgrp(master, own) != 0)
        return !fail("set-own", errno);
    int read_back = tcgetpgrp(master);
    if (read_back != own)
        return !fail("get-own", errno);
    printf("tty-pgrp: set-own pgrp=%d ok=1\n", own);
    fflush(stdout);

    char fd_text[16];
    char pgrp_text[16];
    snprintf(fd_text, sizeof(fd_text), "%d", master);
    snprintf(pgrp_text, sizeof(pgrp_text), "%d", own);

    fflush(stdout);
    pid_t child = fork();
    if (child == 0) {
        char *child_argv[] = {SELF, "child", fd_text, pgrp_text, NULL};
        execve(SELF, child_argv, environ);
        printf("tty-pgrp: child exec failed errno=%d\n", errno);
        fflush(stdout);
        _exit(127);
    }
    if (child < 0 || !wait_child(child))
        return !fail("child-exec", errno);
    printf("tty-pgrp: child-exec ok\n");
    fflush(stdout);

    int nullfd = open("/dev/null", O_RDONLY);
    if (nullfd < 0)
        return !fail("open-null", errno);
    errno = 0;
    if (tcgetpgrp(nullfd) != -1 || errno != ENOTTY)
        return !fail("non-tty-get", errno);
    errno = 0;
    if (tcsetpgrp(nullfd, own) != -1 || errno != ENOTTY)
        return !fail("non-tty-set", errno);
    printf("tty-pgrp: non-tty ENOTTY ok\n");
    fflush(stdout);

    errno = 0;
    if (tcsetpgrp(master, -1) != -1 || errno != EINVAL)
        return !fail("invalid", errno);
    printf("tty-pgrp: invalid EINVAL ok\n");
    fflush(stdout);

    int pipefd[2];
    if (pipe(pipefd) != 0)
        return !fail("pipe", errno);
    fflush(stdout);
    pid_t other = fork();
    if (other == 0) {
        close(pipefd[0]);
        if (setsid() < 0)
            _exit(2);
        int group = getpgrp();
        if (write(pipefd[1], &group, sizeof(group)) != (ssize_t)sizeof(group))
            _exit(3);
        close(pipefd[1]);
        sleep(1);
        _exit(0);
    }
    close(pipefd[1]);
    printf("tty-pgrp: cross-fork pid=%d\n", (int)other);
    fflush(stdout);
    int other_group = 0;
    ssize_t got = read(pipefd[0], &other_group, sizeof(other_group));
    close(pipefd[0]);
    printf("tty-pgrp: cross-read got=%d group=%d\n", (int)got, other_group);
    fflush(stdout);
    if (other < 0 || got != (ssize_t)sizeof(other_group))
        return !fail("other-child", errno);
    errno = 0;
    int cross = tcsetpgrp(master, other_group);
    int cross_errno = errno;
    printf("tty-pgrp: cross-call result=%d errno=%d\n", cross, cross_errno);
    fflush(stdout);
    kill(other, SIGKILL);
    int other_status = 0;
    waitpid(other, &other_status, 0);
    printf("tty-pgrp: cross-reaped\n");
    fflush(stdout);
    if (cross != -1 || cross_errno != EPERM)
        return !fail("cross-session", cross_errno);
    printf("tty-pgrp: cross-session EPERM ok\n");
    fflush(stdout);

    if (tcgetpgrp(master) != own)
        return !fail("setpgid-get", errno);
    printf("tty-pgrp: setpgid+tcsetpgrp ok\n");
    fflush(stdout);

    close(nullfd);
    close(master);
    printf("tty-pgrp: done\n");
    fflush(stdout);
    return 0;
}
