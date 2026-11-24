// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S180 test-only IPC conformance probe. It exercises the POSIX/FreeBSD contract: SCM_RIGHTS with
// the received descriptor usable after the sender closes and with the rights mask honored,
// SO_PEERCRED exact pid/uid/gid, MSG_CMSG_CLOEXEC closing on exec, FIFO blocking open and
// non-blocking behavior, and socketpair message boundaries for SOCK_SEQPACKET vs SOCK_STREAM.
// It changes no product code.

#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/wait.h>
#include <unistd.h>

#ifndef MSG_CMSG_CLOEXEC
#define MSG_CMSG_CLOEXEC 0x40000000
#endif

struct ucred_local {
    pid_t pid;
    uid_t uid;
    gid_t gid;
};

static int failures;

static void row(const char *name, int ok, const char *detail) {
    printf("S180-IPC row=%s status=%s detail=%s\n", name, ok ? "PASS" : "FAIL", detail);
    fflush(stdout);
    if (!ok) {
        failures++;
    }
}

static int send_fd(int sock, int fd) {
    struct msghdr message;
    struct iovec iov;
    char byte = 'X';
    char control[CMSG_SPACE(sizeof(int))];
    memset(&message, 0, sizeof message);
    memset(control, 0, sizeof control);
    iov.iov_base = &byte;
    iov.iov_len = 1;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof control;
    struct cmsghdr *cmsg = CMSG_FIRSTHDR(&message);
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    cmsg->cmsg_len = CMSG_LEN(sizeof(int));
    memcpy(CMSG_DATA(cmsg), &fd, sizeof fd);
    return (int)sendmsg(sock, &message, 0);
}

static int recv_fd(int sock, int flags, int *fd_out) {
    struct msghdr message;
    struct iovec iov;
    char byte = 0;
    char control[CMSG_SPACE(sizeof(int))];
    memset(&message, 0, sizeof message);
    memset(control, 0, sizeof control);
    iov.iov_base = &byte;
    iov.iov_len = 1;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof control;
    if (recvmsg(sock, &message, flags) < 0) {
        return -1;
    }
    struct cmsghdr *cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL || cmsg->cmsg_level != SOL_SOCKET || cmsg->cmsg_type != SCM_RIGHTS ||
        cmsg->cmsg_len < CMSG_LEN(sizeof(int))) {
        return -1;
    }
    memcpy(fd_out, CMSG_DATA(cmsg), sizeof *fd_out);
    return 0;
}

static void test_rights(int mutation) {
    int sv[2];
    int wfd;
    int sfd;
    int rfd = -1;
    char path[64];
    char buffer[8];
    char detail[160];
    snprintf(path, sizeof path, "/tmp/s180-rights-%d", (int)getpid());
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        row("rights", 0, "socketpair failed");
        return;
    }
    wfd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (wfd < 0) {
        row("rights", 0, "open seed failed");
        return;
    }
    (void)write(wfd, "K180", 4);
    close(wfd);
    // The normal sender holds a read-only handle; the mutation drops the rights mask by using a
    // writable handle and the received descriptor writes (the expected read-only row must go red).
    sfd = open(path, mutation ? O_RDWR : O_RDONLY);
    if (sfd < 0) {
        row("rights", 0, "open sender failed");
        return;
    }
    int sent = send_fd(sv[0], sfd);
    close(sfd);
    int received = recv_fd(sv[1], 0, &rfd);
    memset(buffer, 0, sizeof buffer);
    ssize_t got = rfd >= 0 ? read(rfd, buffer, 4) : -1;
    errno = 0;
    ssize_t wrote = rfd >= 0 ? write(rfd, "Y", 1) : -1;
    int write_errno = errno;
    int ok = sent == 1 && received == 0 && got == 4 && memcmp(buffer, "K180", 4) == 0;
    // The sender was closed before the read above: the received descriptor is independent.
    if (ok) {
        ok = mutation ? wrote == 1 : wrote == -1;
    }
    snprintf(detail, sizeof detail, "sent=%d recv=%d read=%zd write=%zd errno=%d mutation=%d",
             sent, received, got, wrote, write_errno, mutation);
    row("rights", ok, detail);
    if (rfd >= 0) {
        close(rfd);
    }
    close(sv[0]);
    close(sv[1]);
    unlink(path);
}

static void test_credentials(void) {
    int sv[2];
    struct ucred_local cred;
    socklen_t length = sizeof cred;
    char detail[128];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        row("credentials", 0, "socketpair failed");
        return;
    }
    memset(&cred, 0, sizeof cred);
    int result = getsockopt(sv[1], SOL_SOCKET, SO_PEERCRED, &cred, &length);
    int ok = result == 0 && cred.pid == getpid() && cred.uid == getuid() && cred.gid == getgid();
    snprintf(detail, sizeof detail, "rc=%d pid=%d/%d uid=%u/%u gid=%u/%u", result, (int)cred.pid,
             (int)getpid(), cred.uid, getuid(), cred.gid, getgid());
    row("credentials", ok, detail);
    close(sv[0]);
    close(sv[1]);
}

static void test_cloexec(const char *program) {
    int sv[2];
    int sfd;
    int rfd = -1;
    char path[64];
    char fd_text[16];
    char detail[160];
    snprintf(path, sizeof path, "/tmp/s180-cloexec-%d", (int)getpid());
    sfd = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (sfd < 0 || socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        row("cloexec", 0, "setup failed");
        return;
    }
    close(sfd);
    sfd = open(path, O_RDONLY);
    (void)send_fd(sv[0], sfd);
    close(sfd);
    int received = recv_fd(sv[1], MSG_CMSG_CLOEXEC, &rfd);
    int fd_flags = rfd >= 0 ? fcntl(rfd, F_GETFD) : -1;
    int cloexec = fd_flags >= 0 && (fd_flags & FD_CLOEXEC) != 0;
    snprintf(fd_text, sizeof fd_text, "%d", rfd);
    pid_t child = fork();
    int status = -1;
    if (child == 0) {
        execl(program, program, "--check-fd", fd_text, (char *)NULL);
        _exit(127);
    }
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    int ok = received == 0 && cloexec && WIFEXITED(status) && WEXITSTATUS(status) == 0;
    snprintf(detail, sizeof detail, "recv=%d fd=%d flags=0x%x cloexec=%d child=%d", received, rfd,
             fd_flags, cloexec, status);
    row("cloexec", ok, detail);
    if (rfd >= 0) {
        close(rfd);
    }
    close(sv[0]);
    close(sv[1]);
    unlink(path);
}

static void test_fifo(void) {
    char path[64];
    char detail[192];
    char byte = 0;
    snprintf(path, sizeof path, "/tmp/s180-fifo-%d", (int)getpid());
    unlink(path);
    if (mkfifo(path, 0600) < 0) {
        row("fifo", 0, "mkfifo failed");
        return;
    }
    int rfd = open(path, O_RDONLY | O_NONBLOCK);
    errno = 0;
    ssize_t read_result = rfd >= 0 ? read(rfd, &byte, 1) : -1;
    int read_errno = errno;
    int wfd = open(path, O_WRONLY | O_NONBLOCK);
    if (wfd >= 0) {
        close(wfd);
    }
    if (rfd >= 0) {
        close(rfd);
    }
    errno = 0;
    int no_reader = open(path, O_WRONLY | O_NONBLOCK);
    int open_errno = errno;
    if (no_reader >= 0) {
        close(no_reader);
    }

    pid_t child = fork();
    int status = -1;
    if (child == 0) {
        int cr = open(path, O_RDONLY);
        if (cr < 0) {
            _exit(2);
        }
        char received = 0;
        if (read(cr, &received, 1) != 1) {
            _exit(3);
        }
        _exit(received == 'F' ? 0 : 4);
    }
    int pw = open(path, O_WRONLY);
    if (pw >= 0) {
        (void)write(pw, "F", 1);
        close(pw);
    }
    if (child > 0) {
        (void)waitpid(child, &status, 0);
    }
    int nonblocking = rfd >= 0 && read_result == -1 &&
                      (read_errno == EAGAIN || read_errno == EWOULDBLOCK) && no_reader == -1 &&
                      (open_errno == ENXIO || open_errno == EAGAIN);
    int blocking = WIFEXITED(status) && WEXITSTATUS(status) == 0;
    snprintf(detail, sizeof detail,
             "reader=%d read=%zd read_errno=%d no_reader=%d open_errno=%d child=%d",
             rfd, read_result, read_errno, no_reader, open_errno, status);
    row("fifo", nonblocking && blocking, detail);
    unlink(path);
}

static void test_seqpacket(void) {
    int sv[2];
    char first[8];
    char second[8];
    char detail[128];
    memset(first, 0, sizeof first);
    memset(second, 0, sizeof second);
    if (socketpair(AF_UNIX, SOCK_SEQPACKET, 0, sv) < 0) {
        row("seqpacket", 0, "socketpair SOCK_SEQPACKET failed");
        return;
    }
    (void)send(sv[0], "A", 1, 0);
    (void)send(sv[0], "B", 1, 0);
    ssize_t n1 = recv(sv[1], first, sizeof first, 0);
    ssize_t n2 = recv(sv[1], second, sizeof second, 0);
    int ok = n1 == 1 && first[0] == 'A' && n2 == 1 && second[0] == 'B';
    snprintf(detail, sizeof detail, "first=%zd second=%zd", n1, n2);
    row("seqpacket", ok, detail);
    close(sv[0]);
    close(sv[1]);
}

static void test_stream(void) {
    int sv[2];
    char buffer[8];
    char detail[128];
    memset(buffer, 0, sizeof buffer);
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) < 0) {
        row("stream", 0, "socketpair SOCK_STREAM failed");
        return;
    }
    (void)send(sv[0], "A", 1, 0);
    (void)send(sv[0], "B", 1, 0);
    ssize_t n = recv(sv[1], buffer, sizeof buffer, 0);
    int ok = n >= 1 && (n == 2 ? (buffer[0] == 'A' && buffer[1] == 'B') : buffer[0] == 'A');
    snprintf(detail, sizeof detail, "recv=%zd first=%02x", n, (unsigned char)buffer[0]);
    row("stream", ok, detail);
    close(sv[0]);
    close(sv[1]);
}

static void test_stream(void);

int main(int argc, char **argv) {
    const char *mode = argc > 1 ? argv[1] : "--all";
    if (strcmp(mode, "--check-fd") == 0) {
        int fd = argc > 2 ? atoi(argv[2]) : -1;
        return fcntl(fd, F_GETFD) < 0 ? 0 : 1;
    }
    int mutation = strcmp(mode, "--mutation-rights") == 0;
    if (strcmp(mode, "--all") != 0 && !mutation) {
        fprintf(stderr, "usage: %s [--all|--mutation-rights|--check-fd FD]\n", argv[0]);
        return 64;
    }
    test_rights(mutation);
    test_credentials();
    test_cloexec(argv[0]);
    test_fifo();
    test_seqpacket();
    test_stream();
    printf("S180-IPC done failures=%d\n", failures);
    fflush(stdout);
    return failures == 0 ? 0 : 1;
}
