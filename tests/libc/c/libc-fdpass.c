/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K438: SCM_RIGHTS fd passing over AF_UNIX, the OpenSSH monitor_fdpass contract.
 *
 * The test sends descriptors with sendmsg + SCM_RIGHTS and receives them on the peer with
 * recvmsg. It covers the three shapes the privsep monitor uses and the control-record negative
 * cases:
 *   - a regular file: write through the received fd, read the same bytes through the original;
 *   - a pty master: write the received fd, read the slave, and the reverse;
 *   - two descriptors in one message;
 *   - a too-small control buffer (EMSGSIZE, message preserved) and a message without control.
 * Every assertion prints the descriptor numbers and the cmsg fields so the qtest serial names the
 * exact layer when one is red. */

#include <errno.h>
#include <fcntl.h>
#include <libutil.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <termios.h>
#include <unistd.h>

static int fail(const char *what) {
    printf("fdpass: fail %s errno=%d\n", what, errno);
    return 1;
}

static ssize_t read_full(int fd, void *buffer, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t got = read(fd, (unsigned char *)buffer + done, length - done);
        if (got < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (got == 0) {
            break;
        }
        done += (size_t)got;
    }
    return (ssize_t)done;
}

static int send_fds(int sock, const int *fds, int count, const void *data, size_t length) {
    struct msghdr message;
    struct iovec iov;
    union {
        struct cmsghdr header;
        char buffer[CMSG_SPACE(2 * sizeof(int))];
    } control;
    struct cmsghdr *cmsg;

    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = (void *)data;
    iov.iov_len = length;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = CMSG_SPACE((size_t)count * sizeof(int));
    cmsg = CMSG_FIRSTHDR(&message);
    cmsg->cmsg_len = CMSG_LEN((size_t)count * sizeof(int));
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    memcpy(CMSG_DATA(cmsg), fds, (size_t)count * sizeof(int));
    return (int)sendmsg(sock, &message, 0);
}

static int recv_fds(int sock, int *fds, int capacity, void *data, size_t data_capacity,
                    size_t *used, size_t *control_len) {
    struct msghdr message;
    struct iovec iov;
    union {
        struct cmsghdr header;
        char buffer[CMSG_SPACE(2 * sizeof(int))];
    } control;
    struct cmsghdr *cmsg;
    ssize_t count;
    int received = 0;

    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = data;
    iov.iov_len = data_capacity;
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = sizeof(control.buffer);
    count = recvmsg(sock, &message, 0);
    if (count < 0) {
        return -1;
    }
    *used = (size_t)count;
    *control_len = (size_t)message.msg_controllen;
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg != NULL) {
        if (cmsg->cmsg_level != SOL_SOCKET || cmsg->cmsg_type != SCM_RIGHTS) {
            errno = EPROTO;
            return -1;
        }
        if (cmsg->cmsg_len < CMSG_LEN(sizeof(int))) {
            errno = EPROTO;
            return -1;
        }
        size_t payload = (size_t)cmsg->cmsg_len - CMSG_LEN(0);
        int count_fds = (int)(payload / sizeof(int));
        if (received + count_fds > capacity) {
            errno = EMSGSIZE;
            return -1;
        }
        memcpy(&fds[received], CMSG_DATA(cmsg), (size_t)count_fds * sizeof(int));
        received += count_fds;
    }
    return received;
}

static int file_phase(void) {
    int original;
    int received = -1;
    int pair[2];
    char buffer[64];
    size_t used = 0;
    size_t control_len = 0;

    original = open("/tmp/fdpass-file", O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (original < 0) {
        return fail("file open");
    }
    if (write(original, "original", 8) != 8) {
        return fail("file original write");
    }
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0) {
        return fail("file socketpair");
    }
    if (send_fds(pair[0], &original, 1, "F", 1) != 1) {
        return fail("file sendmsg");
    }
    if (recv_fds(pair[1], &received, 1, buffer, sizeof(buffer), &used, &control_len) != 1) {
        return fail("file recvmsg");
    }
    if (used != 1 || buffer[0] != 'F') {
        return fail("file data");
    }
    if (received == original) {
        return fail("file fd not independent");
    }
    printf("fdpass: file fd=%d received=%d cmsg_len=%zu controllen=%zu\n", original, received,
           (size_t)CMSG_LEN(sizeof(int)), control_len);
    if (write(received, " passed", 7) != 7) {
        return fail("file received write");
    }
    if (lseek(original, 0, SEEK_SET) != 0) {
        return fail("file seek");
    }
    if (read_full(original, buffer, 15) != 15 || memcmp(buffer, "original passed", 15) != 0) {
        return fail("file roundtrip");
    }
    if (close(received) != 0) {
        return fail("file received close");
    }
    if (lseek(original, 0, SEEK_SET) != 0 || read_full(original, buffer, 15) != 15) {
        return fail("file original after close");
    }
    (void)close(original);
    (void)close(pair[0]);
    (void)close(pair[1]);
    printf("fdpass: file roundtrip=ok\n");
    return 0;
}

static int pty_phase(void) {
    int master = -1;
    int slave = -1;
    int received = -1;
    int pair[2];
    char name[128];
    char buffer[32];
    size_t used = 0;
    size_t control_len = 0;

    if (openpty(&master, &slave, name, NULL, NULL) < 0) {
        return fail("pty openpty");
    }
    {
        struct termios settings;
        if (tcgetattr(slave, &settings) != 0) {
            return fail("pty tcgetattr");
        }
        settings.c_lflag &= ~ECHO;
        if (tcsetattr(slave, TCSANOW, &settings) != 0) {
            return fail("pty tcsetattr");
        }
    }
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0) {
        return fail("pty socketpair");
    }
    if (send_fds(pair[0], &master, 1, "P", 1) != 1) {
        return fail("pty sendmsg");
    }
    if (recv_fds(pair[1], &received, 1, buffer, sizeof(buffer), &used, &control_len) != 1) {
        return fail("pty recvmsg");
    }
    if (used != 1 || buffer[0] != 'P' || received == master) {
        return fail("pty data");
    }
    printf("fdpass: pty master=%d slave=%d received=%d controllen=%zu\n", master, slave, received,
           control_len);
    if (write(received, "pty-passed\n", 11) != 11) {
        return fail("pty received write");
    }
    if (read_full(slave, buffer, 11) != 11 || memcmp(buffer, "pty-passed\n", 11) != 0) {
        return fail("pty slave read");
    }
    if (write(slave, "pty-back\n", 9) != 9) {
        return fail("pty slave write");
    }
    if (read_full(received, buffer, 9) != 9 || memcmp(buffer, "pty-back\n", 9) != 0) {
        return fail("pty received read");
    }
    (void)close(received);
    (void)close(master);
    (void)close(slave);
    (void)close(pair[0]);
    (void)close(pair[1]);
    printf("fdpass: pty roundtrip=ok\n");
    return 0;
}

static int two_fd_phase(void) {
    int first = -1;
    int second = -1;
    int received[2] = {-1, -1};
    int pair[2];
    int passed[2];
    char buffer[64];
    size_t used = 0;
    size_t control_len = 0;

    first = open("/tmp/fdpass-first", O_RDWR | O_CREAT | O_TRUNC, 0600);
    second = open("/tmp/fdpass-second", O_RDWR | O_CREAT | O_TRUNC, 0600);
    if (first < 0 || second < 0) {
        return fail("two open");
    }
    if (write(first, "first", 5) != 5 || write(second, "second", 6) != 6) {
        return fail("two write");
    }
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0) {
        return fail("two socketpair");
    }
    passed[0] = first;
    passed[1] = second;
    if (send_fds(pair[0], passed, 2, "T", 1) != 1) {
        return fail("two sendmsg");
    }
    if (recv_fds(pair[1], received, 2, buffer, sizeof(buffer), &used, &control_len) != 2) {
        return fail("two recvmsg");
    }
    if (used != 1 || buffer[0] != 'T' || received[0] == received[1] ||
        received[0] == first || received[1] == second) {
        return fail("two data");
    }
    printf("fdpass: two fds first=%d second=%d cmsg_len=%zu controllen=%zu\n", received[0],
           received[1], (size_t)CMSG_LEN(2 * sizeof(int)), control_len);
    if (write(received[0], "+a", 2) != 2 || write(received[1], "+b", 2) != 2) {
        return fail("two received write");
    }
    if (lseek(first, 0, SEEK_SET) != 0 || read_full(first, buffer, 7) != 7 ||
        memcmp(buffer, "first+a", 7) != 0) {
        return fail("two first roundtrip");
    }
    if (lseek(second, 0, SEEK_SET) != 0 || read_full(second, buffer, 8) != 8 ||
        memcmp(buffer, "second+b", 8) != 0) {
        return fail("two second roundtrip");
    }
    (void)close(received[0]);
    (void)close(received[1]);
    (void)close(first);
    (void)close(second);
    (void)close(pair[0]);
    (void)close(pair[1]);
    printf("fdpass: two fds roundtrip=ok\n");
    return 0;
}

static int negative_phase(void) {
    int pair[2];
    int passed[2];
    int received = -1;
    char byte = 'N';
    char buffer[8];
    size_t used = 0;
    size_t control_len = 0;
    struct msghdr message;
    struct iovec iov;
    union {
        struct cmsghdr header;
        char buffer[CMSG_SPACE(sizeof(int))];
    } control;

    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0 ||
        socketpair(AF_UNIX, SOCK_STREAM, 0, passed) != 0) {
        return fail("negative socketpair");
    }
    if (send_fds(pair[0], &passed[0], 1, &byte, 1) != 1) {
        return fail("negative send");
    }
    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = buffer;
    iov.iov_len = sizeof(buffer);
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = 8;
    if (recvmsg(pair[1], &message, 0) != -1 || errno != EMSGSIZE) {
        return fail("negative truncation");
    }
    printf("fdpass: truncated errno=%d ok=1\n", EMSGSIZE);
    if (recv_fds(pair[1], &received, 1, buffer, sizeof(buffer), &used, &control_len) != 1) {
        return fail("negative retry");
    }
    (void)close(received);

    if (send(pair[0], "plain", 5, 0) != 5) {
        return fail("negative plain send");
    }
    memset(&message, 0, sizeof(message));
    memset(&control, 0, sizeof(control));
    iov.iov_base = buffer;
    iov.iov_len = sizeof(buffer);
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control.buffer;
    message.msg_controllen = sizeof(control.buffer);
    if (recvmsg(pair[1], &message, 0) != 5 || message.msg_controllen != 0 ||
        CMSG_FIRSTHDR(&message) != NULL || memcmp(buffer, "plain", 5) != 0) {
        return fail("negative absent");
    }
    printf("fdpass: absent controllen=0 ok=1\n");
    (void)close(passed[0]);
    (void)close(passed[1]);
    (void)close(pair[0]);
    (void)close(pair[1]);
    return 0;
}

int main(void) {
    printf("fdpass: start\n");
    if (file_phase() != 0) {
        return 1;
    }
    if (pty_phase() != 0) {
        return 1;
    }
    if (two_fd_phase() != 0) {
        return 1;
    }
    if (negative_phase() != 0) {
        return 1;
    }
    printf("fdpass: ok\n");
    return 0;
}
