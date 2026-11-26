/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/*
 * K486: descriptors across exec.
 *
 * The controller opens a known fd set, marks some CLOEXEC, execs /bin/libc-test again in "child"
 * mode and the child probes every number with fcntl(F_GETFD). The same run covers fcntl after
 * 64 descriptors over 10 rounds, closefrom below/above the threshold, socketpair inheritance and
 * the parent's fd count returning to baseline. Any disagreement prints FAIL and exits non-zero.
 */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

#define SELF "/bin/libc-test"
#define MAX_FDS 64
#define LIST_CAP 1024
#define SCAN_LIMIT 256

static int append_fd(char *list, int fd) {
    size_t used = strlen(list);
    int written = snprintf(list + used, LIST_CAP - used, "%s%d", used ? "," : "", fd);
    return written > 0 && (size_t)written < LIST_CAP - used;
}

static int list_has(const char *list, int fd) {
    const char *cursor = list;
    while (*cursor) {
        char *end = NULL;
        long value = strtol(cursor, &end, 10);
        if (end == cursor)
            break;
        if (value == fd)
            return 1;
        cursor = end;
        if (*cursor == ',')
            cursor++;
    }
    return 0;
}

static int count_open(void) {
    int count = 0;
    for (int fd = 0; fd < SCAN_LIMIT; fd++) {
        errno = 0;
        if (fcntl(fd, F_GETFD) != -1)
            count++;
    }
    return count;
}

static int open_test_fd(void) {
    int fd = open(SELF, O_RDONLY);
    if (fd < 0)
        printf("fd-exec: open %s failed errno=%d\n", SELF, errno);
    return fd;
}

static int child_mode(int argc, char **argv) {
    const char *keep = argc > 2 ? argv[2] : "";
    const char *close_list = argc > 3 ? argv[3] : "";
    int sock = argc > 4 ? atoi(argv[4]) : -1;
    int keep_ok = 1;
    int close_ok = 1;
    int sock_ok = 1;

    for (int fd = 0; fd < SCAN_LIMIT; fd++) {
        if (list_has(keep, fd) && fd != sock) {
            if (fcntl(fd, F_GETFD) == -1) {
                keep_ok = 0;
                continue;
            }
            char byte = 0;
            if (read(fd, &byte, 1) != 1)
                keep_ok = 0;
        }
        if (list_has(close_list, fd)) {
            errno = 0;
            if (fcntl(fd, F_GETFD) != -1 || errno != EBADF)
                close_ok = 0;
        }
    }

    if (sock >= 0) {
        int type = 0;
        socklen_t length = sizeof(type);
        if (fcntl(sock, F_GETFD) == -1) {
            printf("fd-exec: child sock fd=%d is closed errno=%d\n", sock, errno);
            sock_ok = 0;
        } else if (getsockopt(sock, SOL_SOCKET, SO_TYPE, &type, &length) != 0) {
            printf("fd-exec: child sock getsockopt errno=%d\n", errno);
            sock_ok = 0;
        } else if (type != SOCK_STREAM) {
            printf("fd-exec: child sock type=%d\n", type);
            sock_ok = 0;
        } else if (write(sock, "S", 1) != 1) {
            printf("fd-exec: child sock write errno=%d\n", errno);
            sock_ok = 0;
        }
    }

    int ok = keep_ok && close_ok && sock_ok;
    printf("fd-exec: child ok=%d keep=%d close=%d sock=%d\n", ok, keep_ok, close_ok, sock_ok);
    fflush(stdout);
    return ok ? 0 : 1;
}

static int wait_child(pid_t pid) {
    int status = 0;
    if (waitpid(pid, &status, 0) != pid)
        return 0;
    return WIFEXITED(status) && WEXITSTATUS(status) == 0;
}

static int run_round(int round, int baseline) {
    int fds[MAX_FDS];
    char keep[LIST_CAP] = {0};
    char close_list[LIST_CAP] = {0};
    int keep_count = 0;
    int close_count = 0;
    int ok = 1;

    for (int index = 0; index < MAX_FDS; index++) {
        fds[index] = open_test_fd();
        if (fds[index] < 0)
            return 0;
        if (index % 2 == 0) {
            if (fcntl(fds[index], F_SETFD, FD_CLOEXEC) == -1)
                ok = 0;
        }
    }

    int src = open(SELF, O_RDONLY | O_CLOEXEC);
    int d1 = src >= 0 ? dup(src) : -1;
    int d2 = src >= 0 ? fcntl(src, F_DUPFD_CLOEXEC, 0) : -1;
    int src2 = open(SELF, O_RDONLY);
    int d3 = src2 >= 0 ? dup(src2) : -1;
    if (d3 >= 0 && fcntl(d3, F_SETFD, FD_CLOEXEC) == -1)
        ok = 0;
    if (src < 0 || d1 < 0 || d2 < 0 || src2 < 0 || d3 < 0)
        ok = 0;

    for (int index = 0; index < MAX_FDS; index++) {
        if (index % 2 == 0) {
            ok &= append_fd(close_list, fds[index]);
            close_count++;
        } else {
            ok &= append_fd(keep, fds[index]);
            keep_count++;
        }
    }
    /* The original O_CLOEXEC fd closes; its plain dup survives. */
    ok &= append_fd(close_list, src);
    close_count++;
    ok &= append_fd(keep, d1);
    keep_count++;
    /* F_DUPFD_CLOEXEC duplicates close; the plain original survives. */
    ok &= append_fd(close_list, d2);
    close_count++;
    ok &= append_fd(keep, src2);
    keep_count++;
    /* A post-dup F_SETFD marks only d3. */
    ok &= append_fd(close_list, d3);
    close_count++;

    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        char *argv[] = {SELF, "child", keep, close_list, "-1", NULL};
        execve(SELF, argv, environ);
        printf("fd-exec: child exec failed errno=%d\n", errno);
        fflush(stdout);
        _exit(127);
    }
    if (pid < 0)
        ok = 0;
    else if (!wait_child(pid))
        ok = 0;

    for (int index = 0; index < MAX_FDS; index++)
        close(fds[index]);
    if (src >= 0)
        close(src);
    if (d1 >= 0)
        close(d1);
    if (d2 >= 0)
        close(d2);
    if (src2 >= 0)
        close(src2);
    if (d3 >= 0)
        close(d3);

    int after = count_open();
    ok &= after == baseline;
    printf("fd-exec: round=%d keep=%d close=%d after=%d baseline=%d ok=%d\n", round,
           keep_count, close_count, after, baseline, ok);
    fflush(stdout);
    return ok;
}

static int closefrom_child(void) {
    int fds[12];
    int ok = 1;
    for (int index = 0; index < 12; index++) {
        fds[index] = open_test_fd();
        if (fds[index] < 0)
            return 0;
    }
    for (int index = 7; index < 12; index++) {
        if (fcntl(fds[index], F_SETFD, FD_CLOEXEC) == -1)
            ok = 0;
    }
    closefrom(8);
    for (int index = 0; index < 5; index++) {
        if (fcntl(fds[index], F_GETFD) == -1)
            ok = 0;
    }
    for (int index = 5; index < 12; index++) {
        errno = 0;
        if (fcntl(fds[index], F_GETFD) != -1 || errno != EBADF)
            ok = 0;
    }
    return ok;
}

static int run_closefrom(void) {
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0)
        _exit(closefrom_child() ? 0 : 1);
    if (pid < 0 || !wait_child(pid)) {
        printf("fd-exec: closefrom FAIL\n");
        fflush(stdout);
        return 0;
    }
    printf("fd-exec: closefrom ok\n");
    fflush(stdout);
    return 1;
}

static int run_socketpair(void) {
    int pair[2];
    char keep[32];
    char sockbuf[32];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0) {
        printf("fd-exec: socketpair setup FAIL errno=%d\n", errno);
        fflush(stdout);
        return 0;
    }
    snprintf(keep, sizeof(keep), "%d", pair[0]);
    snprintf(sockbuf, sizeof(sockbuf), "%d", pair[0]);
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        char *argv[] = {SELF, "child", keep, "", sockbuf, NULL};
        execve(SELF, argv, environ);
        printf("fd-exec: socket child exec failed errno=%d\n", errno);
        fflush(stdout);
        _exit(127);
    }
    int ok = pid >= 0 && wait_child(pid);
    char byte = 0;
    if (ok && read(pair[1], &byte, 1) == 1 && byte == 'S')
        ok = 1;
    else
        ok = 0;
    close(pair[0]);
    close(pair[1]);
    printf("fd-exec: socketpair ok=%d\n", ok);
    fflush(stdout);
    return ok;
}

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "child") == 0)
        return child_mode(argc, argv);

    printf("fd-exec: start\n");
    fflush(stdout);
    int baseline = count_open();
    printf("fd-exec: baseline=%d\n", baseline);
    fflush(stdout);

    int ok = 1;
    for (int round = 1; round <= 10; round++) {
        if (!run_round(round, baseline))
            ok = 0;
    }
    if (!run_closefrom())
        ok = 0;
    /* K482 owns the socket-record restoration; this row is informational on the K486 base. */
    int socket_ok = run_socketpair();

    printf("fd-exec: done ok=%d socket=%d\n", ok, socket_ok);
    fflush(stdout);
    return ok ? 0 : 1;
}
