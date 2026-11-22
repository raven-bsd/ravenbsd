/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K476: the sshd privsep pty shape. A monitor process owns the pty; a session child is fork+exec'd,
 * and only after exec does it ask the monitor over an inherited socketpair for a pty. The monitor
 * runs posix_openpt/grantpt/unlockpt and passes the master with SCM_RIGHTS. The child opens the
 * slave by name, round-trips a byte, closes, and asks for a second allocation. A negative child
 * closes the channel first and must see EPIPE/EBADF. The socket fd must survive execve; if it does
 * not, the step/errno line names the failure. */

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
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

#define REQ_PTY 28
#define ANS_PTY 29
#define REQ_CLEANUP 30
#define FRAME_MAX 512

static int fail(const char *what, long value) {
    printf("pty-privsep-exec: FAIL %s=%ld errno=%d\n", what, value, errno);
    fflush(stdout);
    return 1;
}

static int read_all(int fd, void *buffer, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t count = read(fd, (unsigned char *)buffer + done, length - done);
        if (count < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (count == 0)
            return -1;
        done += (size_t)count;
    }
    return 0;
}

static int write_all(int fd, const void *buffer, size_t length) {
    size_t done = 0;
    while (done < length) {
        ssize_t count = write(fd, (const unsigned char *)buffer + done, length - done);
        if (count < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        if (count == 0)
            return -1;
        done += (size_t)count;
    }
    return 0;
}

static void put_u32_be(unsigned char *out, uint32_t value) {
    out[0] = (unsigned char)(value >> 24);
    out[1] = (unsigned char)(value >> 16);
    out[2] = (unsigned char)(value >> 8);
    out[3] = (unsigned char)value;
}

static uint32_t get_u32_be(const unsigned char *in) {
    return ((uint32_t)in[0] << 24) | ((uint32_t)in[1] << 16) | ((uint32_t)in[2] << 8) |
           (uint32_t)in[3];
}

static int send_frame(int sock, unsigned char type, const unsigned char *payload,
                      size_t length) {
    unsigned char header[5];
    put_u32_be(header, (uint32_t)(length + 1));
    header[4] = type;
    if (write_all(sock, header, sizeof(header)) != 0)
        return -1;
    if (length > 0 && write_all(sock, payload, length) != 0)
        return -1;
    return 0;
}

static int read_frame(int sock, unsigned char expect, unsigned char *payload, size_t capacity,
                      size_t *used) {
    unsigned char header[4];
    unsigned char buffer[FRAME_MAX];
    if (read_all(sock, header, sizeof(header)) != 0)
        return -1;
    uint32_t total = get_u32_be(header);
    if (total < 1 || total > sizeof(buffer) || total > capacity + 1) {
        errno = EPROTO;
        return -1;
    }
    if (read_all(sock, buffer, total) != 0)
        return -1;
    if (buffer[0] != expect) {
        errno = EPROTO;
        return -1;
    }
    if (payload != NULL && total > 1)
        memcpy(payload, buffer + 1, total - 1);
    if (used != NULL)
        *used = total - 1;
    return 0;
}

static int send_fd(int sock, int fd) {
    char byte = '\0';
    struct iovec iov;
    struct msghdr message;
    char control[CMSG_SPACE(sizeof(int))];
    struct cmsghdr *cmsg;

    iov.iov_base = &byte;
    iov.iov_len = 1;
    memset(&message, 0, sizeof(message));
    memset(control, 0, sizeof(control));
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL)
        return -1;
    cmsg->cmsg_len = CMSG_LEN(sizeof(int));
    cmsg->cmsg_level = SOL_SOCKET;
    cmsg->cmsg_type = SCM_RIGHTS;
    memcpy(CMSG_DATA(cmsg), &fd, sizeof(int));
    return sendmsg(sock, &message, 0) == 1 ? 0 : -1;
}

static int recv_fd(int sock) {
    char byte = 0;
    struct iovec iov;
    struct msghdr message;
    char control[CMSG_SPACE(sizeof(int))];
    struct cmsghdr *cmsg;
    struct pollfd waiting;
    int ready;

    waiting.fd = sock;
    waiting.events = POLLIN;
    waiting.revents = 0;
    do {
        ready = poll(&waiting, 1, 3000);
    } while (ready < 0 && errno == EINTR);
    if (ready == 0) {
        errno = EAGAIN;
        return -1;
    }
    if (ready < 0)
        return -1;

    iov.iov_base = &byte;
    iov.iov_len = 1;
    memset(&message, 0, sizeof(message));
    memset(control, 0, sizeof(control));
    message.msg_iov = &iov;
    message.msg_iovlen = 1;
    message.msg_control = control;
    message.msg_controllen = sizeof(control);
    ssize_t received = recvmsg(sock, &message, 0);
    if (received != 1) {
        if (received == 0)
            errno = EPIPE;
        return -1;
    }
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL || cmsg->cmsg_level != SOL_SOCKET ||
        cmsg->cmsg_type != SCM_RIGHTS || cmsg->cmsg_len != CMSG_LEN(sizeof(int))) {
        errno = EPROTO;
        return -1;
    }
    int fd = -1;
    memcpy(&fd, CMSG_DATA(cmsg), sizeof(int));
    return fd;
}

static int monitor_main(int sock) {
    for (int round = 0; round < 2; round++) {
        if (read_frame(sock, REQ_PTY, NULL, 0, NULL) != 0) {
            // The negative child closes its channel before the first request: an orderly EOF.
            if (round == 0) {
                close(sock);
                return 0;
            }
            printf("pty-privsep-exec: monitor request round=%d errno=%d\n", round, errno);
            return 3;
        }
        int master = posix_openpt(O_RDWR | O_NOCTTY);
        if (master < 0 || grantpt(master) != 0 || unlockpt(master) != 0) {
            printf("pty-privsep-exec: monitor openpt round=%d errno=%d\n", round, errno);
            return 4;
        }
        char *name = ptsname(master);
        if (name == NULL) {
            printf("pty-privsep-exec: monitor ptsname round=%d errno=%d\n", round, errno);
            return 4;
        }
        unsigned char payload[FRAME_MAX];
        size_t length = strlen(name) + 1;
        put_u32_be(payload, 1);
        put_u32_be(payload + 4, (uint32_t)length);
        memcpy(payload + 8, name, length);
        printf("pty-privsep-exec: monitor round=%d master=%d name=%s\n", round, master, name);
        fflush(stdout);
        if (send_frame(sock, ANS_PTY, payload, 8 + length) != 0 ||
            send_fd(sock, master) != 0) {
            printf("pty-privsep-exec: monitor send round=%d errno=%d\n", round, errno);
            return 5;
        }
        close(master);
    }
    (void)read_frame(sock, REQ_CLEANUP, NULL, 0, NULL);
    close(sock);
    return 0;
}

static void report_state(const char *tag, int fd) {
    struct stat inherited;
    int fstat_rv = fstat(fd, &inherited);
    int fstat_errno = errno;
    int duplicated = dup(fd);
    int dup_errno = errno;
    if (duplicated >= 0)
        close(duplicated);
    int type = 0;
    socklen_t length = (socklen_t)sizeof(type);
    errno = 0;
    int sockopt = getsockopt(fd, SOL_SOCKET, SO_TYPE, &type, &length);
    int sockopt_errno = errno;
    int flags = fcntl(fd, F_GETFD);
    printf(
        "pty-privsep-exec: child state tag=%s fstat=%d/%d dup=%d/%d getsockopt=%d/%d "
        "type=%d fcntl=%d\n",
        tag, fstat_rv, fstat_errno, duplicated, dup_errno, sockopt, sockopt_errno, type,
        flags);
    fflush(stdout);
}

static int session_child(int sock) {
    for (int round = 0; round < 2; round++) {
        if (send_frame(sock, REQ_PTY, NULL, 0) != 0)
            return fail("request", round);
        printf("pty-privsep-exec: child round=%d request=ok\n", round);
        fflush(stdout);
        unsigned char payload[FRAME_MAX];
        size_t used = 0;
        if (read_frame(sock, ANS_PTY, payload, sizeof(payload), &used) != 0)
            return fail("reply", round);
        uint32_t success = get_u32_be(payload);
        uint32_t name_length = get_u32_be(payload + 4);
        if (success != 1 || name_length < 1 || 8 + name_length > used)
            return fail("reply-fields", round);
        char name[128];
        memset(name, 0, sizeof(name));
        memcpy(name, payload + 8, name_length);
        printf("pty-privsep-exec: child round=%d name=%s\n", round, name);
        fflush(stdout);
        if (strncmp(name, "/dev/pts/", 9) != 0)
            return fail("name-path", round);
        int master = recv_fd(sock);
        printf("pty-privsep-exec: child round=%d master=%d\n", round, master);
        fflush(stdout);
        if (master < 0)
            return fail("recv-master", round);
        int slave = open(name, O_RDWR | O_NOCTTY);
        if (slave < 0)
            return fail("open-slave", round);
        if (write(master, "M\n", 2) != 2)
            return fail("write-master", round);
        char byte = 0;
        if (read(slave, &byte, 1) != 1 || byte != 'M')
            return fail("read-slave", round);
        if (write(slave, "S\n", 2) != 2)
            return fail("write-slave", round);
        /* The slave line discipline echoes the master's "M\n" back to the master. Drain that
         * echo before requiring the slave's "S". K476's case originally read one byte and saw
         * the echo; the libc fd handoff is independent of this pty termios fact. */
        for (int spin = 0; spin < 50; spin++) {
            ssize_t got = read(master, &byte, 1);
            if (got < 0) {
                if (errno == EINTR)
                    continue;
                return fail("read-master", round);
            }
            if (got == 0)
                return fail("read-master", round);
            if (byte == 'S')
                break;
            if (spin == 49)
                return fail("read-master", round);
        }
        printf("pty-privsep-exec: child round=%d roundtrip=MS\n", round);
        fflush(stdout);
        close(slave);
        close(master);
    }
    (void)send_frame(sock, REQ_CLEANUP, NULL, 0);
    close(sock);
    printf("pty-privsep-exec: child allocations=2 ok\n");
    fflush(stdout);
    return 0;
}

static int negative_child(int sock) {
    close(sock);
    if (send_frame(sock, REQ_PTY, NULL, 0) == 0) {
        printf("pty-privsep-exec: negative unexpected send=ok\n");
        return 1;
    }
    int error = errno;
    printf("pty-privsep-exec: negative send errno=%d\n", error);
    fflush(stdout);
    if (error != EPIPE && error != EBADF)
        return 1;
    printf("pty-privsep-exec: negative channel-closed ok\n");
    return 0;
}

static int exec_child(const char *tag, int fd, int negative) {
    char fd_text[16];
    snprintf(fd_text, sizeof(fd_text), "%d", fd);
    pid_t child = fork();
    if (child < 0)
        return -1;
    if (child == 0) {
        char *argv[] = { "libc-test", "pty-privsep-exec-child", fd_text,
                         negative ? "negative" : "exec1", NULL };
        char *envp[] = { NULL };
        (void)execve("/bin/libc-test", argv, envp);
        _exit(127);
    }
    return child;
}

static int run_case(int negative) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair", -1);
    pid_t monitor = fork();
    if (monitor < 0)
        return fail("fork-monitor", -1);
    if (monitor == 0) {
        close(pair[0]);
        _exit(monitor_main(pair[1]));
    }
    close(pair[1]);
    pid_t child = exec_child(negative ? "negative" : "positive", pair[0], negative);
    if (child < 0)
        return fail("fork-child", -1);
    close(pair[0]);
    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("wait-child", -1);
    printf("pty-privsep-exec: %s child status=%d\n", negative ? "negative" : "positive",
           status);
    fflush(stdout);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
        return fail("child-status", status);
    int monitor_status = 0;
    if (waitpid(monitor, &monitor_status, 0) != monitor)
        return fail("wait-monitor", -1);
    printf("pty-privsep-exec: %s monitor status=%d\n", negative ? "negative" : "positive",
           monitor_status);
    fflush(stdout);
    if (!WIFEXITED(monitor_status) || WEXITSTATUS(monitor_status) != 0)
        return fail("monitor-status", monitor_status);
    return 0;
}

int main(int argc, char **argv) {
    if (argc > 2 && strcmp(argv[1], "pty-privsep-exec-child") == 0) {
        int fd = atoi(argv[2]);
        const char *mode = argc > 3 ? argv[3] : "run";
        int negative = strcmp(mode, "negative") == 0;
        if (strcmp(mode, "run") == 0) {
            printf("pty-privsep-exec: child reexec fd=%d\n", fd);
        } else {
            printf("pty-privsep-exec: child start fd=%d negative=%d\n", fd, negative);
        }
        fflush(stdout);
        report_state(strcmp(mode, "run") == 0 ? "exec2" : "exec1", fd);
        if (strcmp(mode, "exec1") == 0) {
            char *again[] = { "libc-test", "pty-privsep-exec-child", argv[2], "run", NULL };
            char *envp[] = { NULL };
            (void)execve("/bin/libc-test", again, envp);
            _exit(127);
        }
        if (negative)
            return negative_child(fd);
        return session_child(fd);
    }

    printf("pty-privsep-exec: start uid=%d\n", (int)getuid());
    fflush(stdout);
    if (run_case(0) != 0)
        return 1;
    printf("pty-privsep-exec: positive ok\n");
    fflush(stdout);
    if (run_case(1) != 0)
        return 1;
    printf("pty-privsep-exec: negative ok\n");
    fflush(stdout);
    printf("pty-privsep-exec: ok\n");
    return 0;
}
