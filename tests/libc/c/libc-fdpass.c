/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K438: SCM_RIGHTS fd passing over AF_UNIX. The parent sends one/two fds over a socketpair; the
 * forked child receives them (the OpenSSH monitor shape), asserts the cmsg fields and uses the
 * received fd: regular file byte identity and a pty master byte on the slave. A malformed cmsg
 * and an absent cmsg are the negative cases. */

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <libutil.h>
#include <netinet/in.h>
#include <poll.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/wait.h>
#include <unistd.h>

static int fail(const char *what, int value) {
    printf("fdpass: fail %s=%d errno=%d\n", what, value, errno);
    fflush(stdout);
    return 1;
}

static int send_fds(int sock, const int *fds, int count, int *sent_cmsg_len) {
    char byte = 'x';
    struct iovec iov;
    struct msghdr message;
    char control[CMSG_SPACE(sizeof(int) * 2)];
    struct cmsghdr *cmsg;

    iov.iov_base = &byte;
    iov.iov_len = 1;
    memset(&message, 0, sizeof(message));
    memset(control, 0, sizeof(control));
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = CMSG_SPACE(sizeof(int) * count);
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL)
        return fail("cmsg-first", 0);
    cmsg->cmsg_len = CMSG_LEN(sizeof(int) * count);
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    memcpy(CMSG_DATA(cmsg), fds, sizeof(int) * (size_t)count);
    *sent_cmsg_len = (int)cmsg->cmsg_len;
    ssize_t written = sendmsg(sock, &message, 0);
    if (written != 1)
        return fail("sendmsg", (int)written);
    return 0;
}

static int recv_fds(int sock, int *fds, int max, const char *who) {
    char byte = 0;
    struct iovec iov;
    struct msghdr message;
    char control[CMSG_SPACE(sizeof(int) * 2)];
    struct cmsghdr *cmsg;

    iov.iov_base = &byte;
    iov.iov_len = 1;
    memset(&message, 0, sizeof(message));
    memset(control, 0, sizeof(control));
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    ssize_t received = recvmsg(sock, &message, 0);
    if (received != 1)
        return fail("recvmsg", (int)received);
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL) {
        printf("fdpass: %s cmsg=absent controllen=%u\n", who, (unsigned)message.msg_controllen);
        return 0;
    }
    int count = (int)((cmsg->cmsg_len - CMSG_LEN(0)) / sizeof(int));
    printf(
        "fdpass: %s cmsg_len=%u level=%d type=%d count=%d controllen=%u\n",
        who,
        (unsigned)cmsg->cmsg_len,
        (int)cmsg->cmsg_level,
        (int)cmsg->cmsg_type,
        count,
        (unsigned)message.msg_controllen
    );
    if (cmsg->cmsg_level != SOL_SOCKET || cmsg->cmsg_type != SCM_RIGHTS)
        return fail("cmsg-fields", 0);
    if (count < 1 || count > max)
        return fail("cmsg-count", count);
    memcpy(fds, CMSG_DATA(cmsg), sizeof(int) * (size_t)count);
    if (count == 1)
        printf("fdpass: %s fd0=%d\n", who, fds[0]);
    else
        printf("fdpass: %s fd0=%d fd1=%d\n", who, fds[0], fds[1]);
    for (int index = 0; index < count; index++) {
        if (fds[index] < 0)
            return fail("cmsg-fd", fds[index]);
        for (int other = 0; other < index; other++)
            if (fds[other] == fds[index])
                return fail("cmsg-fd-duplicate", fds[index]);
        if (fds[index] == sock)
            return fail("cmsg-fd-socket", fds[index]);
    }
    return count;
}

#ifndef FDPASS_TCP

static int child_use_file(int fd) {
    if (lseek(fd, 0, SEEK_SET) < 0)
        return fail("child-file-lseek", -1);
    if (write(fd, "child", 5) != 5)
        return fail("child-file-write", -1);
    return 0;
}

static int child_use_pty(int fd) {
    const char *text = "pty-child\n";
    if (write(fd, text, strlen(text)) != (ssize_t)strlen(text))
        return fail("child-pty-write", -1);
    return 0;
}

static int pass_file(void) {
    const char *path = "/tmp/fdpass-file";
    int original = open(path, O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (original < 0)
        return fail("file-open", -1);
    if (write(original, "orig", 4) != 4)
        return fail("file-write", -1);

    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-file", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-file", -1);
    if (child == 0) {
        close(pair[0]);
        int received[2];
        int count = recv_fds(pair[1], received, 2, "file-child");
        if (count != 1)
            _exit(fail("child-file-count", count) != 0 ? 1 : 1);
        if (child_use_file(received[0]) != 0)
            _exit(1);
        close(received[0]);
        fflush(stdout);
        _exit(0);
    }
    close(pair[1]);
    int sent_len = 0;
    if (send_fds(pair[0], &original, 1, &sent_len) != 0)
        return fail("file-send", -1);
    printf("fdpass: file sent=1 sent_cmsg_len=%d\n", sent_len);
    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("file-wait", -1);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("file-child-status", status);

    char buffer[16];
    memset(buffer, 0, sizeof(buffer));
    if (lseek(original, 0, SEEK_SET) < 0)
        return fail("file-lseek-parent", -1);
    ssize_t got = read(original, buffer, sizeof(buffer) - 1);
    if (got != 5 || memcmp(buffer, "child", 5) != 0)
        return fail("file-identity", (int)got);
    printf("fdpass: file parent-read=%s\n", buffer);
    close(pair[0]);
    close(original);
    unlink(path);
    return 0;
}

static int pass_pty(void) {
    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (master < 0)
        return fail("pty-openpt", -1);
    if (grantpt(master) != 0 || unlockpt(master) != 0)
        return fail("pty-grant", -1);
    char *name = ptsname(master);
    if (name == NULL)
        return fail("pty-ptsname", -1);
    int slave = open(name, O_RDWR);
    if (slave < 0)
        return fail("pty-slave-open", -1);
    printf("fdpass: pty master=%d slave=%d name=%s\n", master, slave, name);

    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-pty", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-pty", -1);
    if (child == 0) {
        close(pair[0]);
        close(slave);
        int received[2];
        int count = recv_fds(pair[1], received, 2, "pty-child");
        if (count != 1)
            _exit(1);
        if (child_use_pty(received[0]) != 0)
            _exit(1);
        fflush(stdout);
        _exit(0);
    }
    close(pair[1]);
    int sent_len = 0;
    if (send_fds(pair[0], &master, 1, &sent_len) != 0)
        return fail("pty-send", -1);
    printf("fdpass: pty sent=1 sent_cmsg_len=%d\n", sent_len);
    close(master);
    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("pty-wait", -1);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("pty-child-status", status);

    char buffer[64];
    size_t total = 0;
    for (int spins = 0; spins < 2000 && total < 10; spins++) {
        ssize_t got = read(slave, buffer + total, sizeof(buffer) - 1 - total);
        if (got > 0) {
            total += (size_t)got;
            continue;
        }
        if (got == 0)
            break;
        if (errno == EAGAIN || errno == EINTR)
            continue;
        return fail("pty-read", (int)got);
    }
    buffer[total] = 0;
    printf("fdpass: pty parent-read=%s\n", buffer);
    if (strstr(buffer, "pty-child") == NULL)
        return fail("pty-identity", (int)total);
    close(pair[0]);
    close(slave);
    return 0;
}

static int pass_two(void) {
    int file = open("/tmp/fdpass-two", O_RDWR | O_CREAT | O_TRUNC, 0600);
    int master = posix_openpt(O_RDWR | O_NOCTTY);
    if (file < 0 || master < 0)
        return fail("two-open", -1);
    if (grantpt(master) != 0 || unlockpt(master) != 0)
        return fail("two-grant", -1);
    char *name = ptsname(master);
    int slave = name == NULL ? -1 : open(name, O_RDWR);
    if (slave < 0)
        return fail("two-slave", -1);
    int fds[2] = {file, master};

    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-two", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-two", -1);
    if (child == 0) {
        close(pair[0]);
        close(slave);
        int received[2];
        int count = recv_fds(pair[1], received, 2, "two-child");
        if (count != 2)
            _exit(1);
        if (child_use_file(received[0]) != 0)
            _exit(1);
        if (child_use_pty(received[1]) != 0)
            _exit(1);
        fflush(stdout);
        _exit(0);
    }
    close(pair[1]);
    int sent_len = 0;
    if (send_fds(pair[0], fds, 2, &sent_len) != 0)
        return fail("two-send", -1);
    printf("fdpass: two sent=2 sent_cmsg_len=%d\n", sent_len);
    close(master);
    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("two-wait", -1);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("two-child-status", status);

    char buffer[16];
    memset(buffer, 0, sizeof(buffer));
    if (lseek(file, 0, SEEK_SET) < 0)
        return fail("two-file-lseek", -1);
    ssize_t got = read(file, buffer, sizeof(buffer) - 1);
    if (got != 5 || memcmp(buffer, "child", 5) != 0)
        return fail("two-file-identity", (int)got);
    char pty[64];
    size_t total = 0;
    for (int spins = 0; spins < 2000 && total < 10; spins++) {
        ssize_t count = read(slave, pty + total, sizeof(pty) - 1 - total);
        if (count > 0) {
            total += (size_t)count;
            continue;
        }
        if (count == 0)
            break;
        if (errno == EAGAIN || errno == EINTR)
            continue;
        return fail("two-pty-read", (int)count);
    }
    pty[total] = 0;
    if (strstr(pty, "pty-child") == NULL)
        return fail("two-pty-identity", (int)total);
    printf("fdpass: two file=child pty=pty-child\n");
    close(pair[0]);
    close(file);
    close(slave);
    unlink("/tmp/fdpass-two");
    return 0;
}

static int truncated_control(void) {
    int original = open("/tmp/fdpass-trunc", O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (original < 0)
        return fail("trunc-open", -1);
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-trunc", -1);
    int sent_len = 0;
    if (send_fds(pair[0], &original, 1, &sent_len) != 0)
        return fail("trunc-send", -1);

    char byte = 0;
    struct iovec iov;
    struct msghdr message;
    char control[4];
    iov.iov_base = &byte;
    iov.iov_len = 1;
    memset(&message, 0, sizeof(message));
    memset(control, 0, sizeof(control));
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    ssize_t received = recvmsg(pair[1], &message, 0);
    int truncated_errno = received < 0 ? errno : 0;
    printf("fdpass: truncated recv=%d errno=%d\n", (int)received, truncated_errno);
    if (received >= 0 || truncated_errno != EMSGSIZE)
        return fail("truncated", (int)received);

    // The too-small control buffer must leave the message queued, so a retry with a full
    // buffer receives the same byte and fd.
    int received_fds[2];
    int count = recv_fds(pair[1], received_fds, 2, "truncated-retry");
    if (count != 1)
        return fail("truncated-retry-count", count);
    close(received_fds[0]);
    close(pair[0]);
    close(pair[1]);
    close(original);
    unlink("/tmp/fdpass-trunc");
    return 0;
}

static int malformed_and_absent(void) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-neg", -1);

    char byte = 'x';
    struct iovec iov;
    struct msghdr message;
    char control[8];
    iov.iov_base = &byte;
    iov.iov_len = 1;
    memset(&message, 0, sizeof(message));
    memset(control, 0, sizeof(control));
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    struct cmsghdr *cmsg = (struct cmsghdr *)control;
    cmsg->cmsg_len = sizeof(control);
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    ssize_t sent = sendmsg(pair[0], &message, 0);
    int malformed_errno = sent < 0 ? errno : 0;
    printf("fdpass: malformed sent=%d errno=%d\n", (int)sent, malformed_errno);
    if (sent >= 0 || malformed_errno != EINVAL)
        return fail("malformed", (int)sent);

    char plain = 'y';
    struct iovec plain_iov;
    struct msghdr plain_message;
    plain_iov.iov_base = &plain;
    plain_iov.iov_len = 1;
    memset(&plain_message, 0, sizeof(plain_message));
    plain_message.msg_iov = &plain_iov;
    plain_message.msg_iovlen = 1;
    if (sendmsg(pair[0], &plain_message, 0) != 1)
        return fail("absent-send", -1);

    char received_byte = 0;
    struct iovec recv_iov;
    struct msghdr recv_message;
    char recv_control[CMSG_SPACE(sizeof(int))];
    recv_iov.iov_base = &received_byte;
    recv_iov.iov_len = 1;
    memset(&recv_message, 0, sizeof(recv_message));
    memset(recv_control, 0, sizeof(recv_control));
    recv_message.msg_iov = &recv_iov;
    recv_message.msg_iovlen = 1;
    recv_message.msg_control = recv_control;
    recv_message.msg_controllen = sizeof(recv_control);
    if (recvmsg(pair[1], &recv_message, 0) != 1)
        return fail("absent-recv", -1);
    printf("fdpass: absent control=%u\n", (unsigned)recv_message.msg_controllen);
    if (recv_message.msg_controllen != 0)
        return fail("absent-control", (int)recv_message.msg_controllen);
    close(pair[0]);
    close(pair[1]);
    return 0;
}

#else /* FDPASS_TCP */

#define TCP_PORT 23084
#define TCP_HOST "10.0.2.2"
#define TCP_PAYLOAD "fdpass-tcp-child"
#define TCP_ACK "fdpass-tcp-ack"

static ssize_t read_all(int fd, void *buffer, size_t count) {
    size_t done = 0;
    while (done < count) {
        ssize_t got = read(fd, (unsigned char *)buffer + done, count - done);
        if (got < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (got == 0)
            break;
        done += (size_t)got;
    }
    return (ssize_t)done;
}

static ssize_t write_all(int fd, const void *buffer, size_t count) {
    size_t done = 0;
    while (done < count) {
        ssize_t wrote = write(fd, (const unsigned char *)buffer + done, count - done);
        if (wrote < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        done += (size_t)wrote;
    }
    return (ssize_t)done;
}

static int pass_tcp(void) {
    struct sockaddr_in address;
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_port = htons(TCP_PORT);
    if (inet_pton(AF_INET, TCP_HOST, &address.sin_addr) != 1)
        return fail("tcp-address", -1);

    int connection = socket(AF_INET, SOCK_STREAM, 0);
    if (connection < 0)
        return fail("tcp-socket", -1);
    int attempt = 0;
    for (; attempt < 100; attempt++) {
        if (connect(connection, (const struct sockaddr *)&address, sizeof(address)) == 0)
            break;
        usleep(100000);
    }
    if (attempt == 100)
        return fail("tcp-connect", -1);
    printf("fdpass: tcp connected fd=%d\n", connection);

    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-tcp", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-tcp", -1);
    if (child == 0) {
        // Drop every inherited copy: the only descriptor the child may use is the one the
        // kernel installs from the SCM_RIGHTS message.
        close(pair[0]);
        close(connection);
        int received[2];
        int count = recv_fds(pair[1], received, 2, "tcp-child");
        if (count != 1)
            _exit(1);
        const char *payload = TCP_PAYLOAD;
        size_t payload_length = strlen(payload);
        if (write_all(received[0], payload, payload_length) != (ssize_t)payload_length)
            _exit(1);
        char ack[64];
        memset(ack, 0, sizeof(ack));
        ssize_t got = read_all(received[0], ack, strlen(TCP_ACK));
        if (got != (ssize_t)strlen(TCP_ACK)) {
            printf("fdpass: tcp-child ack-read=%d errno=%d\n", (int)got, errno);
            fflush(stdout);
            _exit(1);
        }
        printf("fdpass: tcp-child ack=%s\n", ack);
        fflush(stdout);
        close(received[0]);
        close(pair[1]);
        _exit(0);
    }
    close(pair[1]);
    int sent_len = 0;
    if (send_fds(pair[0], &connection, 1, &sent_len) != 0)
        return fail("tcp-send", -1);
    printf("fdpass: tcp sent=1 sent_cmsg_len=%d\n", sent_len);
    close(connection);
    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("tcp-wait", -1);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("tcp-child-status", status);
    close(pair[0]);
    printf("fdpass: tcp child-status=0\n");
    return 0;
}

#endif /* FDPASS_TCP */

int main(void) {
    printf("fdpass: start\n");
#ifdef FDPASS_TCP
    if (pass_tcp() != 0)
        return 1;
#else
    if (pass_file() != 0)
        return 1;
    if (pass_pty() != 0)
        return 1;
    if (pass_two() != 0)
        return 1;
    if (truncated_control() != 0)
        return 1;
    if (malformed_and_absent() != 0)
        return 1;
#endif
    printf("fdpass: ok\n");
    return 0;
}
