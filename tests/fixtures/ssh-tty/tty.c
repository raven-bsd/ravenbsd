// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K434 S8 test-only `tty`: the release profile has no tty utility. This fixture prints the POSIX
// ttyname of stdin so `ssh -tt ... tty` can assert /dev/pts/N. It also prints the fd/stdin/stdio
// evidence used to separate sshd's do_exec_pty dup2 from a shell replacing its stdio. It is test
// tooling, not base code.

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static void print_link(const char *label, int fd) {
    char path[64];
    char target[256];
    (void)snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
    ssize_t count = readlink(path, target, sizeof(target) - 1);
    if (count < 0) {
        printf("TTY-PROBE %s=(readlink errno=%d)\n", label, errno);
        return;
    }
    target[count] = '\0';
    printf("TTY-PROBE %s=%s\n", label, target);
}

static void print_fdinfo(const char *label, int fd) {
    char path[64];
    char buffer[512];
    (void)snprintf(path, sizeof(path), "/proc/self/fdinfo/%d", fd);
    int file = open(path, O_RDONLY);
    if (file < 0) {
        printf("TTY-PROBE %s=(open errno=%d)\n", label, errno);
        return;
    }
    ssize_t count = read(file, buffer, sizeof(buffer) - 1);
    (void)close(file);
    if (count <= 0) {
        printf("TTY-PROBE %s=(read errno=%d)\n", label, errno);
        return;
    }
    buffer[count] = '\0';
    char *flags = strstr(buffer, "flags:");
    if (flags == NULL) {
        printf("TTY-PROBE %s=no-flags\n", label);
        return;
    }
    char *end = strchr(flags, '\n');
    if (end != NULL) {
        *end = '\0';
    }
    printf("TTY-PROBE %s=%s\n", label, flags);
}

int main(void) {
    char *name = ttyname(STDIN_FILENO);
    char *name_out = ttyname(STDOUT_FILENO);
    char *name_err = ttyname(STDERR_FILENO);

    printf("TTY-PROBE pid=%d\n", (int)getpid());
    printf("TTY-PROBE isatty0=%d isatty1=%d isatty2=%d\n", isatty(STDIN_FILENO),
           isatty(STDOUT_FILENO), isatty(STDERR_FILENO));
    print_link("fd0", STDIN_FILENO);
    print_link("fd1", STDOUT_FILENO);
    print_link("fd2", STDERR_FILENO);
    print_fdinfo("fdinfo0", STDIN_FILENO);
    printf("TTY-PROBE ttyname0=%s\n", name == NULL ? "(none)" : name);
    printf("TTY-PROBE ttyname1=%s\n", name_out == NULL ? "(none)" : name_out);
    printf("TTY-PROBE ttyname2=%s\n", name_err == NULL ? "(none)" : name_err);
    int controlling = open("/dev/tty", O_RDWR | O_NOCTTY);
    if (controlling < 0) {
        printf("TTY-PROBE devtty=open-fail errno=%d\n", errno);
    } else {
        printf("TTY-PROBE devtty=open-ok isatty=%d\n", isatty(controlling));
        (void)close(controlling);
    }

    if (name == NULL) {
        fprintf(stderr, "tty: not a tty: %s\n", strerror(errno));
        return 1;
    }
    printf("%s\n", name);
    return 0;
}
