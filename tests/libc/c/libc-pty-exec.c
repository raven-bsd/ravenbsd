// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K548 rev333: the exact sshd do_exec_pty stdio sequence, with a PRE/POST exec report.
//
// The child opens a pty pair, becomes a session leader, takes the slave as the controlling
// terminal (the login_tty/ioctl TIOCSCTTY step), dup2s the slave to 0/1/2, reports isatty and the
// fstat type through the slave, then execve's the same binary. The post image reports the same
// fields again. The parent reads the master and prints the child transcript, so the first step
// that stops being a tty is named by data.

#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>

static void report(const char *phase) {
    struct stat status;
    int fstat_rv = fstat(0, &status);
    const char *name = ttyname(0);
    char line[256];
    int length = snprintf(line, sizeof(line),
                          "PTY-EXEC %s isatty=%d/%d/%d fstat=%d type=%o tty=%s\n", phase,
                          isatty(0), isatty(1), isatty(2), fstat_rv,
                          fstat_rv == 0 ? (unsigned)(status.st_mode & S_IFMT) : 0u,
                          name != NULL ? name : "-");
    if (length > 0) {
        (void)write(1, line, (size_t)length);
    }
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "post") == 0) {
        report("POST");
        return 0;
    }
    printf("PTY-EXEC start uid=%d\n", (int)getuid());
    fflush(stdout);

    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0) {
        printf("PTY-EXEC FAIL posix_openpt errno=%d\n", errno);
        return 1;
    }
    if (grantpt(master) != 0 || unlockpt(master) != 0) {
        printf("PTY-EXEC FAIL grant-unlock errno=%d\n", errno);
        return 1;
    }
    char *path = ptsname(master);
    if (path == NULL) {
        printf("PTY-EXEC FAIL ptsname errno=%d\n", errno);
        return 1;
    }
    int slave = open(path, O_RDWR | O_NOCTTY);
    if (slave < 0) {
        printf("PTY-EXEC FAIL open slave errno=%d\n", errno);
        return 1;
    }
    printf("PTY-EXEC pair master=%d slave=%d path=%s\n", master, slave, path);
    fflush(stdout);

    pid_t child = fork();
    if (child == 0) {
        (void)setsid();
        (void)ioctl(slave, TIOCSCTTY, 0);
        (void)dup2(slave, 0);
        (void)dup2(slave, 1);
        (void)dup2(slave, 2);
        if (slave > 2) {
            (void)close(slave);
        }
        (void)close(master);
        report("PRE");
        char *args[] = {"libc-test", "post", NULL};
        char *env[] = {NULL};
        (void)execve("/bin/libc-test", args, env);
        _exit(127);
    }
    if (child < 0) {
        printf("PTY-EXEC FAIL fork errno=%d\n", errno);
        return 1;
    }
    (void)close(slave);
    (void)fcntl(master, F_SETFL, O_NONBLOCK);

    char buffer[1024];
    size_t total = 0;
    int eof = 0;
    for (int spins = 0; spins < 4000 && total + 1 < sizeof(buffer); spins++) {
        ssize_t count = read(master, buffer + total, sizeof(buffer) - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            continue;
        }
        if (count == 0) {
            eof = 1;
            break;
        }
        if (errno == EAGAIN || errno == EWOULDBLOCK) {
            usleep(1000);
            continue;
        }
        if (errno == EINTR) {
            continue;
        }
        break;
    }
    buffer[total] = '\0';
    printf("%s", buffer);
    fflush(stdout);

    int status = 0;
    (void)waitpid(child, &status, 0);
    printf("PTY-EXEC SUMMARY eof=%d exit=%d\n", eof, WIFEXITED(status) ? WEXITSTATUS(status) : -1);
    fflush(stdout);
    return 0;
}
