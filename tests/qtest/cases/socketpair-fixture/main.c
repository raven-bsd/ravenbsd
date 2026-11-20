// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K206 target fixture: exercise the real libc socketpair(AF_UNIX) path over the kernel duplex
// endpoint pair (K208). It prints a single diagnostic line per check on the RavenBSD debug
// console, so the qtest can match it without a tty.

#include <errno.h>
#include <stddef.h>
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
    int length = snprintf(line, sizeof(line), "socketpair: %s errno=%d\n", what, error);
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
    say("socketpair: fail\n");
    return 1;
}

static int send_fd(int sock, int fd, const void *data, size_t len) {
    struct msghdr message;
    struct iovec iov;
    union {
        struct cmsghdr header;
        char buffer[CMSG_SPACE(sizeof(int))];
    } control;
    struct cmsghdr *cmsg;

    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = (void *)data;
    iov.iov_len = len;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = sizeof(control.buffer);
    cmsg = CMSG_FIRSTHDR(&message);
    cmsg->cmsg_len = CMSG_LEN(sizeof(int));
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    memcpy(CMSG_DATA(cmsg), &fd, sizeof(fd));
    return (int)sendmsg(sock, &message, 0);
}

static int recv_fd(int sock, void *data, size_t capacity, size_t *used) {
    struct msghdr message;
    struct iovec iov;
    union {
        struct cmsghdr header;
        char buffer[CMSG_SPACE(sizeof(int))];
    } control;
    struct cmsghdr *cmsg;
    ssize_t count;
    int fd = -1;

    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = data;
    iov.iov_len = capacity;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = sizeof(control.buffer);
    count = recvmsg(sock, &message, 0);
    if (count < 0) {
        return -1;
    }
    *used = (size_t)count;
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL || cmsg->cmsg_level != SOL_SOCKET ||
        cmsg->cmsg_type != SCM_RIGHTS ||
        cmsg->cmsg_len != CMSG_LEN(sizeof(int))) {
        errno = EPROTO;
        return -1;
    }
    memcpy(&fd, CMSG_DATA(cmsg), sizeof(fd));
    return fd;
}

static int dup_pair(void) {
    int sv[2];
    int duplicate;
    char buffer[8];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("dup socketpair");
    }
    duplicate = dup(sv[0]);
    if (duplicate < 0) {
        return fail("dup");
    }
    if (close(sv[0]) != 0) {
        return fail("dup close original");
    }
    if (write(duplicate, "dup", 3) != 3) {
        return fail("dup write");
    }
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 3 || memcmp(buffer, "dup", 3) != 0) {
        return fail("dup read");
    }
    if (close(sv[1]) != 0) {
        return fail("dup close peer");
    }
    count = read(duplicate, buffer, sizeof(buffer));
    if (count != 0) {
        return fail("dup eof");
    }
    if (close(duplicate) != 0) {
        return fail("dup close duplicate");
    }
    say("socketpair: dup ok\n");
    return 0;
}

static int fork_pair(void) {
    int sv[2];
    pid_t child;
    char buffer[8];
    ssize_t count;
    int status = 0;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("fork socketpair");
    }
    child = fork();
    if (child < 0) {
        return fail("fork");
    }
    if (child == 0) {
        (void)close(sv[1]);
        if (write(sv[0], "fork", 4) != 4) {
            _exit(2);
        }
        _exit(0);
    }
    (void)close(sv[0]);
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 4 || memcmp(buffer, "fork", 4) != 0) {
        return fail("fork read");
    }
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("fork eof");
    }
    if (waitpid(child, &status, 0) != child || !WIFEXITED(status) ||
        WEXITSTATUS(status) != 0) {
        return fail("fork wait");
    }
    if (close(sv[1]) != 0) {
        return fail("fork close");
    }
    say("socketpair: fork ok\n");
    return 0;
}

static int control_pair(void) {
    int sv[2];
    int passed[2];
    int received;
    char byte = 'B';
    char buffer[8];
    size_t used = 0;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0 ||
        socketpair(AF_UNIX, SOCK_STREAM, 0, passed) != 0) {
        return fail("control socketpair");
    }
    if (send_fd(sv[0], passed[0], &byte, 1) != 1) {
        return fail("control sendmsg");
    }
    received = recv_fd(sv[1], buffer, sizeof(buffer), &used);
    if (received < 0) {
        return fail("control recvmsg");
    }
    if (used != 1 || buffer[0] != 'B') {
        return fail("control bytes");
    }
    if (close(passed[0]) != 0) {
        return fail("control close original");
    }
    if (write(received, "ok", 2) != 2) {
        return fail("control write received");
    }
    if (read(passed[1], buffer, 2) != 2 || memcmp(buffer, "ok", 2) != 0) {
        return fail("control read peer");
    }
    if (close(received) != 0) {
        return fail("control close received");
    }
    if (read(passed[1], buffer, sizeof(buffer)) != 0) {
        return fail("control eof");
    }
    (void)close(passed[1]);
    (void)close(sv[0]);
    (void)close(sv[1]);
    say("socketpair-control: fd ok\n");
    return 0;
}

static int control_datagram(void) {
    int sv[2];
    int passed[2];
    int received;
    char byte = 'D';
    char buffer[8];
    size_t used = 0;

    if (socketpair(AF_UNIX, SOCK_DGRAM, 0, sv) != 0 ||
        socketpair(AF_UNIX, SOCK_STREAM, 0, passed) != 0) {
        return fail("control dgram socketpair");
    }
    if (send_fd(sv[0], passed[0], &byte, 1) != 1) {
        return fail("control dgram sendmsg");
    }
    received = recv_fd(sv[1], buffer, sizeof(buffer), &used);
    if (received < 0) {
        return fail("control dgram recvmsg");
    }
    if (used != 1 || buffer[0] != 'D') {
        return fail("control dgram bytes");
    }
    (void)close(passed[0]);
    if (write(received, "dg", 2) != 2) {
        return fail("control dgram write");
    }
    if (read(passed[1], buffer, 2) != 2 || memcmp(buffer, "dg", 2) != 0) {
        return fail("control dgram read");
    }
    (void)close(received);
    (void)close(passed[1]);
    (void)close(sv[0]);
    (void)close(sv[1]);
    say("socketpair-control: dgram ok\n");
    return 0;
}

static int control_negative(void) {
    int sv[2];
    int passed[2];
    int dups[17];
    int index;
    char byte = 'N';
    char buffer[8];
    size_t used = 0;
    struct msghdr message;
    struct iovec iov;
    union {
        struct cmsghdr header;
        char buffer[CMSG_SPACE(17 * sizeof(int))];
    } control;
    struct cmsghdr *cmsg;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0 ||
        socketpair(AF_UNIX, SOCK_STREAM, 0, passed) != 0) {
        return fail("control negative socketpair");
    }

    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = &byte;
    iov.iov_len = 1;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = 8;
    if (sendmsg(sv[0], &message, 0) != -1 || errno != EINVAL) {
        return fail("control malformed");
    }

    memset(&control, 0, sizeof(control));
    message.msg_control = control.buffer;
    message.msg_controllen = sizeof(control.buffer);
    cmsg = CMSG_FIRSTHDR(&message);
    cmsg->cmsg_len = CMSG_LEN(sizeof(int));
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS + 1;
    memcpy(CMSG_DATA(cmsg), &passed[0], sizeof(int));
    if (sendmsg(sv[0], &message, 0) != -1 || errno != EOPNOTSUPP) {
        return fail("control unsupported type");
    }

    for (index = 0; index < 17; index++) {
        dups[index] = dup(passed[0]);
        if (dups[index] < 0) {
            return fail("control dup");
        }
    }
    memset(&control, 0, sizeof(control));
    message.msg_control = control.buffer;
    message.msg_controllen = sizeof(control.buffer);
    cmsg = CMSG_FIRSTHDR(&message);
    cmsg->cmsg_len = CMSG_LEN(17 * sizeof(int));
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    memcpy(CMSG_DATA(cmsg), dups, 17 * sizeof(int));
    if (sendmsg(sv[0], &message, 0) != -1 || errno != EMSGSIZE) {
        return fail("control oversized");
    }
    for (index = 0; index < 17; index++) {
        (void)close(dups[index]);
    }

    if (send_fd(sv[0], passed[0], &byte, 1) != 1) {
        return fail("control retry send");
    }
    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = buffer;
    iov.iov_len = sizeof(buffer);
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = 8;
    if (recvmsg(sv[1], &message, 0) != -1 || errno != EMSGSIZE) {
        return fail("control small buffer");
    }
    {
        int received = recv_fd(sv[1], buffer, sizeof(buffer), &used);
        if (received < 0) {
            return fail("control retry recv");
        }
        (void)close(received);
    }
    (void)close(passed[0]);
    (void)close(passed[1]);
    (void)close(sv[0]);
    (void)close(sv[1]);
    say("socketpair-control: negative ok\n");
    return 0;
}

static int peercred_pair(void) {
    int sv[2];
    int inet;
    struct ucred first;
    struct ucred second;
    socklen_t length;
    uid_t uid = 0;
    gid_t gid = 0;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("peercred socketpair");
    }
    memset(&first, 0, sizeof(first));
    length = sizeof(first);
    if (getsockopt(sv[0], SOL_SOCKET, SO_PEERCRED, &first, &length) != 0) {
        return fail("peercred getsockopt");
    }
    if (length != sizeof(first) || first.pid != getpid() ||
        first.uid != getuid() || first.gid != getgid()) {
        return fail("peercred values");
    }
    memset(&second, 0, sizeof(second));
    length = sizeof(second);
    if (getsockopt(sv[1], SOL_SOCKET, SO_PEERCRED, &second, &length) != 0 ||
        memcmp(&first, &second, sizeof(first)) != 0) {
        return fail("peercred both ends");
    }
    if (getpeereid(sv[0], &uid, &gid) != 0 || uid != getuid() || gid != getgid()) {
        return fail("getpeereid");
    }
    inet = socket(AF_INET, SOCK_STREAM, 0);
    if (inet >= 0) {
        memset(&first, 0, sizeof(first));
        length = sizeof(first);
        if (getsockopt(inet, SOL_SOCKET, SO_PEERCRED, &first, &length) != -1 ||
            errno != EOPNOTSUPP) {
            return fail("peercred inet");
        }
        if (getpeereid(inet, &uid, &gid) != -1 || errno != EOPNOTSUPP) {
            return fail("getpeereid inet");
        }
        (void)close(inet);
    }
    (void)close(sv[0]);
    (void)close(sv[1]);
    say("socketpair-control: cred ok\n");
    return 0;
}

static int close_pair(void) {
    int sv[2];

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("close socketpair");
    }
    if (close(sv[0]) != 0) {
        return fail("close first");
    }
    if (close(sv[1]) != 0) {
        return fail("close second");
    }
    say("socketpair: close ok\n");
    return 0;
}

static int stream_pair(void) {
    int sv[2];
    char buffer[16];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("socketpair");
    }

    if (write(sv[0], "ping", 4) != 4) {
        return fail("a-to-b write");
    }
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 4 || memcmp(buffer, "ping", 4) != 0) {
        return fail("a-to-b read");
    }
    say("socketpair: a-to-b ok\n");

    if (write(sv[1], "pong", 4) != 4) {
        return fail("b-to-a write");
    }
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != 4 || memcmp(buffer, "pong", 4) != 0) {
        return fail("b-to-a read");
    }
    say("socketpair: b-to-a ok\n");

    if (shutdown(sv[0], SHUT_WR) != 0) {
        return fail("shutdown write");
    }
    count = read(sv[1], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("peer eof");
    }
    say("socketpair: eof ok\n");

    if (close(sv[0]) != 0) {
        return fail("close a");
    }
    count = send(sv[1], "x", 1, MSG_NOSIGNAL);
    if (count != -1 || errno != EPIPE) {
        return fail("peer close");
    }
    say("socketpair: peer-close ok\n");

    if (close(sv[1]) != 0) {
        return fail("close b");
    }
    return 0;
}

static int shutdown_read_pair(void) {
    int sv[2];
    char buffer[4];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("shutdown-read socketpair");
    }
    if (shutdown(sv[0], SHUT_RD) != 0) {
        return fail("shutdown read");
    }
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("shutdown read eof");
    }
    count = send(sv[1], "x", 1, MSG_NOSIGNAL);
    if (count != -1 || errno != EPIPE) {
        return fail("shutdown read peer epipe");
    }
    say("socketpair: shutdown-read ok\n");
    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}

static int shutdown_both_pair(void) {
    int sv[2];
    char buffer[4];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, sv) != 0) {
        return fail("shutdown-both socketpair");
    }
    if (shutdown(sv[0], SHUT_RDWR) != 0) {
        return fail("shutdown both");
    }
    count = read(sv[0], buffer, sizeof(buffer));
    if (count != 0) {
        return fail("shutdown both eof");
    }
    count = send(sv[0], "x", 1, MSG_NOSIGNAL);
    if (count != -1 || errno != EPIPE) {
        return fail("shutdown both epipe");
    }
    say("socketpair: shutdown-both ok\n");
    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}

static int datagram_pair(void) {
    int sv[2];
    char buffer[16];
    ssize_t count;

    if (socketpair(AF_UNIX, SOCK_DGRAM, 0, sv) != 0) {
        return fail("dgram socketpair");
    }
    if (send(sv[0], "one", 3, 0) != 3 || send(sv[0], "two", 3, 0) != 3) {
        return fail("dgram write");
    }
    count = recv(sv[1], buffer, sizeof(buffer), 0);
    if (count != 3 || memcmp(buffer, "one", 3) != 0) {
        return fail("dgram first");
    }
    count = recv(sv[1], buffer, sizeof(buffer), 0);
    if (count != 3 || memcmp(buffer, "two", 3) != 0) {
        return fail("dgram second");
    }
    say("socketpair: dgram ok\n");
    (void)close(sv[0]);
    (void)close(sv[1]);
    return 0;
}

int main(void) {
    if (datagram_pair() != 0) {
        return 1;
    }
    if (shutdown_read_pair() != 0) {
        return 1;
    }
    if (shutdown_both_pair() != 0) {
        return 1;
    }
    if (dup_pair() != 0) {
        return 1;
    }
    if (fork_pair() != 0) {
        return 1;
    }
    if (close_pair() != 0) {
        return 1;
    }
    if (stream_pair() != 0) {
        return 1;
    }
    if (control_pair() != 0) {
        return 1;
    }
    if (control_datagram() != 0) {
        return 1;
    }
    if (control_negative() != 0) {
        return 1;
    }
    if (peercred_pair() != 0) {
        return 1;
    }
    say("socketpair-control: ok\n");
    say("socketpair: ok\n");
    return 0;
}
