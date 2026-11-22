/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* K442: the OpenSSH privsep PTY exchange, with a stub monitor over socketpair(AF_UNIX).
 *
 * Framing matches OpenSSH's monitor_fdpass/monitor shape:
 *   request/answer: 4-byte big-endian length, 1-byte type, payload; the length includes the type.
 *   PTY request: MONITOR_REQ_PTY (28), empty payload.
 *   PTY answer:  MONITOR_ANS_PTY (29), payload success(u32), name(cstring), msg(cstring), then
 *                four test-only identity fields: master dev/ino and slave dev/ino (u64, BE).
 *   fds:          two separate mm_send_fd-style messages (master, then slave), each one data byte
 *                 plus one SCM_RIGHTS cmsg.
 *
 * The positive case asserts the cmsg fields, the name, the fd identities (fstat dev/ino against
 * the monitor's own master/slave), opening the slave by name, and a master/slave byte round-trip.
 * Three negative cases must be detected: the monitor drops the master fd, the monitor sends a
 * different pty's name (the opened slave does not match the passed slave), and the session omits
 * the request type byte. */

#include <errno.h>
#include <fcntl.h>
#include <libutil.h>
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

#define MONITOR_REQ_PTY 28
#define MONITOR_ANS_PTY 29
#define MONITOR_REQ_PTYCLEANUP 30

#define FRAME_MAX 512

enum monitor_mode {
    MODE_PTY = 0,
    MODE_DROP_FD = 1,
    MODE_WRONG_NAME = 2,
    MODE_SHORT_REQUEST = 3,
};

static int fail(const char *what, long value) {
    printf("pty-monitor: fail %s=%ld errno=%d\n", what, value, errno);
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

static void put_u64_be(unsigned char *out, uint64_t value) {
    for (int index = 7; index >= 0; index--) {
        out[index] = (unsigned char)value;
        value >>= 8;
    }
}

static uint64_t get_u64_be(const unsigned char *in) {
    uint64_t value = 0;
    for (int index = 0; index < 8; index++)
        value = (value << 8) | in[index];
    return value;
}

static int send_frame(int sock, unsigned char type, const unsigned char *payload, size_t length) {
    unsigned char header[5];
    put_u32_be(header, (uint32_t)(length + 1));
    header[4] = type;
    if (write_all(sock, header, sizeof(header)) != 0)
        return -1;
    if (length > 0 && write_all(sock, payload, length) != 0)
        return -1;
    return 0;
}

static int read_frame(
    int sock,
    unsigned char expect,
    unsigned char *payload,
    size_t capacity,
    size_t *used
) {
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
        printf("pty-monitor: frame type=%u expect=%u\n", buffer[0], expect);
        errno = EPROTO;
        return -1;
    }
    if (payload != NULL && total > 1)
        memcpy(payload, buffer + 1, total - 1);
    if (used != NULL)
        *used = total - 1;
    return 0;
}

static int send_fd(int sock, int fd, const char *who) {
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
    ssize_t written = sendmsg(sock, &message, 0);
    if (written != 1)
        return -1;
    printf("pty-monitor: monitor send %s cmsg_len=%u\n", who, (unsigned)cmsg->cmsg_len);
    return 0;
}

static int recv_fd(int sock, const char *who) {
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
        ready = poll(&waiting, 1, 2000);
    } while (ready < 0 && errno == EINTR);
    if (ready == 0) {
        printf("pty-monitor: %s recv=timeout\n", who);
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
    if (received == 0) {
        printf("pty-monitor: %s recv=0\n", who);
        errno = EPIPE;
        return -1;
    }
    if (received != 1) {
        printf("pty-monitor: %s recv=%d errno=%d\n", who, (int)received, errno);
        return -1;
    }
    cmsg = CMSG_FIRSTHDR(&message);
    if (cmsg == NULL) {
        printf("pty-monitor: %s cmsg=absent controllen=%u\n", who,
               (unsigned)message.msg_controllen);
        errno = EPROTO;
        return -1;
    }
    printf(
        "pty-monitor: session %s cmsg_len=%u level=%d type=%d count=%d controllen=%u\n",
        who,
        (unsigned)cmsg->cmsg_len,
        (int)cmsg->cmsg_level,
        (int)cmsg->cmsg_type,
        (int)((cmsg->cmsg_len - CMSG_LEN(0)) / sizeof(int)),
        (unsigned)message.msg_controllen
    );
    if (cmsg->cmsg_level != SOL_SOCKET || cmsg->cmsg_type != SCM_RIGHTS) {
        errno = EPROTO;
        return -1;
    }
    if (cmsg->cmsg_len != CMSG_LEN(sizeof(int))) {
        errno = EPROTO;
        return -1;
    }
    int fd = -1;
    memcpy(&fd, CMSG_DATA(cmsg), sizeof(int));
    if (fd < 0) {
        errno = EBADF;
        return -1;
    }
    return fd;
}

static int open_pty_pair(int *master, int *slave, char *name, size_t capacity) {
    int opened = posix_openpt(O_RDWR | O_NOCTTY);
    if (opened < 0)
        return -1;
    if (grantpt(opened) != 0 || unlockpt(opened) != 0) {
        close(opened);
        return -1;
    }
    char *path = ptsname(opened);
    if (path == NULL) {
        close(opened);
        return -1;
    }
    strncpy(name, path, capacity - 1);
    name[capacity - 1] = 0;
    int peer = open(name, O_RDWR | O_NOCTTY);
    if (peer < 0) {
        close(opened);
        return -1;
    }
    *master = opened;
    *slave = peer;
    return 0;
}

static int read_until_byte(int fd, char want, int spins) {
    struct pollfd waiting;
    for (int spin = 0; spin < spins; spin++) {
        waiting.fd = fd;
        waiting.events = POLLIN;
        waiting.revents = 0;
        int ready = poll(&waiting, 1, 100);
        if (ready > 0 && (waiting.revents & POLLIN) != 0) {
            char byte = 0;
            ssize_t count = read(fd, &byte, 1);
            if (count == 1) {
                if (byte == want)
                    return 0;
                continue;
            }
            if (count < 0 && (errno == EINTR || errno == EAGAIN))
                continue;
            return -1;
        }
        if (ready < 0 && errno != EINTR)
            return -1;
    }
    errno = ETIMEDOUT;
    return -1;
}

static int monitor_main(int sock, enum monitor_mode mode) {
    if (mode == MODE_SHORT_REQUEST) {
        unsigned char header[4];
        unsigned char type = 0;
        if (read_all(sock, header, sizeof(header)) != 0) {
            printf("pty-monitor: monitor short-request header\n");
            return 2;
        }
        ssize_t count = read(sock, &type, 1);
        if (count <= 0) {
            printf("pty-monitor: monitor short-request type\n");
            return 2;
        }
        printf("pty-monitor: monitor unexpected request type=%u\n", type);
        return 3;
    }

    if (read_frame(sock, MONITOR_REQ_PTY, NULL, 0, NULL) != 0) {
        printf("pty-monitor: monitor request read failed errno=%d\n", errno);
        return 3;
    }
    printf("pty-monitor: monitor request type=%u\n", MONITOR_REQ_PTY);

    int master = -1;
    int slave = -1;
    char name[128] = {0};
    int second_master = -1;
    int second_slave = -1;
    char second_name[128] = {0};

    if (open_pty_pair(&master, &slave, name, sizeof(name)) != 0) {
        printf("pty-monitor: monitor openpt failed errno=%d\n", errno);
        return 4;
    }
    printf("pty-monitor: monitor pty master=%d slave=%d name=%s\n", master, slave, name);

    const char *reply_name = name;
    if (mode == MODE_WRONG_NAME) {
        if (open_pty_pair(
                &second_master,
                &second_slave,
                second_name,
                sizeof(second_name)
            ) != 0) {
            printf("pty-monitor: monitor second openpt failed errno=%d\n", errno);
            return 4;
        }
        reply_name = second_name;
        printf("pty-monitor: monitor wrong-name=%s\n", reply_name);
    }

    struct stat master_stat;
    struct stat slave_stat;
    if (fstat(master, &master_stat) != 0 || fstat(slave, &slave_stat) != 0) {
        printf("pty-monitor: monitor fstat failed errno=%d\n", errno);
        return 5;
    }

    unsigned char payload[FRAME_MAX];
    size_t offset = 0;
    put_u32_be(payload + offset, 1);
    offset += 4;
    size_t name_length = strlen(reply_name) + 1;
    put_u32_be(payload + offset, (uint32_t)name_length);
    offset += 4;
    memcpy(payload + offset, reply_name, name_length);
    offset += name_length;
    const char *login = "k442-login";
    size_t login_length = strlen(login) + 1;
    put_u32_be(payload + offset, (uint32_t)login_length);
    offset += 4;
    memcpy(payload + offset, login, login_length);
    offset += login_length;
    put_u64_be(payload + offset, (uint64_t)master_stat.st_dev);
    offset += 8;
    put_u64_be(payload + offset, (uint64_t)master_stat.st_ino);
    offset += 8;
    put_u64_be(payload + offset, (uint64_t)slave_stat.st_dev);
    offset += 8;
    put_u64_be(payload + offset, (uint64_t)slave_stat.st_ino);
    offset += 8;

    if (send_frame(sock, MONITOR_ANS_PTY, payload, offset) != 0) {
        printf("pty-monitor: monitor reply failed errno=%d\n", errno);
        return 6;
    }
    printf("pty-monitor: monitor reply type=%u name=%s\n", MONITOR_ANS_PTY, reply_name);

    if (mode == MODE_DROP_FD) {
        /* The reply is complete, but the master is never sent. */
        close(master);
        close(slave);
        close(sock);
        return 0;
    }

    if (send_fd(sock, master, "master") != 0 || send_fd(sock, slave, "slave") != 0) {
        printf("pty-monitor: monitor send fds failed errno=%d\n", errno);
        return 7;
    }

    if (read_frame(sock, MONITOR_REQ_PTYCLEANUP, NULL, 0, NULL) != 0) {
        printf("pty-monitor: monitor cleanup read failed errno=%d\n", errno);
        return 8;
    }
    printf("pty-monitor: monitor cleanup type=%u\n", MONITOR_REQ_PTYCLEANUP);

    close(master);
    close(slave);
    if (second_master >= 0)
        close(second_master);
    if (second_slave >= 0)
        close(second_slave);
    close(sock);
    return 0;
}

static int wait_monitor(pid_t child, int expected) {
    int status = 0;
    if (waitpid(child, &status, 0) != child) {
        errno = ECHILD;
        return -1;
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != expected) {
        printf("pty-monitor: monitor status=0x%x expected=%d\n", status, expected);
        errno = EPROTO;
        return -1;
    }
    return 0;
}

static int run_positive(void) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-positive", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-positive", -1);
    if (child == 0) {
        close(pair[0]);
        _exit(monitor_main(pair[1], MODE_PTY));
    }
    close(pair[1]);

    if (send_frame(pair[0], MONITOR_REQ_PTY, NULL, 0) != 0)
        return fail("request", -1);
    printf("pty-monitor: session request type=%u\n", MONITOR_REQ_PTY);

    unsigned char payload[FRAME_MAX];
    size_t used = 0;
    if (read_frame(pair[0], MONITOR_ANS_PTY, payload, sizeof(payload), &used) != 0)
        return fail("reply", -1);
    uint32_t success = get_u32_be(payload);
    uint32_t name_length = get_u32_be(payload + 4);
    if (success != 1 || name_length < 1 || name_length > 128)
        return fail("reply-fields", (long)success);
    char name[128] = {0};
    memcpy(name, payload + 8, name_length);
    size_t login_offset = 8 + name_length;
    if (login_offset + 4 > used)
        return fail("reply-name", (long)name_length);
    uint32_t login_length = get_u32_be(payload + login_offset);
    if (login_length < 1 || login_offset + 4 + login_length + 32 > used)
        return fail("reply-msg", (long)login_length);
    printf("pty-monitor: session reply type=%u success=%u\n", MONITOR_ANS_PTY, success);
    printf("pty-monitor: session name=%s msg=%s\n", name, payload + login_offset + 4);
    if (strncmp(name, "/dev/pts/", 9) != 0)
        return fail("reply-name-path", -1);

    size_t identity = login_offset + 4 + login_length;
    uint64_t master_dev = get_u64_be(payload + identity);
    uint64_t master_ino = get_u64_be(payload + identity + 8);
    uint64_t slave_dev = get_u64_be(payload + identity + 16);
    uint64_t slave_ino = get_u64_be(payload + identity + 24);

    int master = recv_fd(pair[0], "master");
    if (master < 0)
        return fail("recv-master", -1);
    int slave = recv_fd(pair[0], "slave");
    if (slave < 0)
        return fail("recv-slave", -1);

    struct stat master_stat;
    struct stat slave_stat;
    if (fstat(master, &master_stat) != 0)
        return fail("fstat-master", -1);
    if ((uint64_t)master_stat.st_dev != master_dev ||
        (uint64_t)master_stat.st_ino != master_ino)
        return fail("master-identity", (long)master_stat.st_ino);
    printf(
        "pty-monitor: session master fd=%d dev=%llu ino=%llu\n",
        master,
        (unsigned long long)master_stat.st_dev,
        (unsigned long long)master_stat.st_ino
    );
    if (fstat(slave, &slave_stat) != 0)
        return fail("fstat-slave", -1);
    if ((uint64_t)slave_stat.st_dev != slave_dev || (uint64_t)slave_stat.st_ino != slave_ino)
        return fail("slave-identity", (long)slave_stat.st_ino);
    if (master_stat.st_ino == 0 || slave_stat.st_ino == 0 ||
        master_stat.st_ino == slave_stat.st_ino)
        return fail("inode-distinct", (long)master_stat.st_ino);
    printf(
        "pty-monitor: session slave fd=%d dev=%llu ino=%llu\n",
        slave,
        (unsigned long long)slave_stat.st_dev,
        (unsigned long long)slave_stat.st_ino
    );

    int by_name = open(name, O_RDWR | O_NOCTTY);
    if (by_name < 0)
        return fail("open-slave-name", -1);
    struct stat by_name_stat;
    if (fstat(by_name, &by_name_stat) != 0)
        return fail("fstat-by-name", -1);
    if ((uint64_t)by_name_stat.st_dev != slave_dev ||
        (uint64_t)by_name_stat.st_ino != slave_ino)
        return fail("by-name-identity", (long)by_name_stat.st_ino);
    printf(
        "pty-monitor: session slave-by-name fd=%d dev=%llu ino=%llu\n",
        by_name,
        (unsigned long long)by_name_stat.st_dev,
        (unsigned long long)by_name_stat.st_ino
    );

    if (write_all(master, "M\n", 2) != 0)
        return fail("write-master", -1);
    if (read_until_byte(by_name, 'M', 50) != 0)
        return fail("read-slave", -1);
    printf("pty-monitor: roundtrip master->slave=M\n");
    if (write_all(by_name, "S\n", 2) != 0)
        return fail("write-slave", -1);
    if (read_until_byte(master, 'S', 50) != 0)
        return fail("read-master", -1);
    printf("pty-monitor: roundtrip slave->master=S\n");

    if (send_frame(pair[0], MONITOR_REQ_PTYCLEANUP, NULL, 0) != 0)
        return fail("cleanup", -1);
    printf("pty-monitor: session cleanup type=%u\n", MONITOR_REQ_PTYCLEANUP);
    if (wait_monitor(child, 0) != 0)
        return fail("monitor-status", -1);

    close(by_name);
    close(slave);
    close(master);
    close(pair[0]);
    return 0;
}

static int run_drop_fd(void) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-drop", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-drop", -1);
    if (child == 0) {
        close(pair[0]);
        _exit(monitor_main(pair[1], MODE_DROP_FD));
    }
    close(pair[1]);

    if (send_frame(pair[0], MONITOR_REQ_PTY, NULL, 0) != 0)
        return fail("drop-request", -1);
    unsigned char payload[FRAME_MAX];
    if (read_frame(pair[0], MONITOR_ANS_PTY, payload, sizeof(payload), NULL) != 0)
        return fail("drop-reply", -1);
    int master = recv_fd(pair[0], "drop-master");
    if (master >= 0) {
        close(master);
        return fail("drop-fd-not-detected", master);
    }
    printf("pty-monitor: drop-fd=detected\n");
    if (wait_monitor(child, 0) != 0)
        return fail("drop-monitor-status", -1);
    close(pair[0]);
    return 0;
}

static int run_wrong_name(void) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-wrong", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-wrong", -1);
    if (child == 0) {
        close(pair[0]);
        _exit(monitor_main(pair[1], MODE_WRONG_NAME));
    }
    close(pair[1]);

    if (send_frame(pair[0], MONITOR_REQ_PTY, NULL, 0) != 0)
        return fail("wrong-request", -1);
    unsigned char payload[FRAME_MAX];
    size_t used = 0;
    if (read_frame(pair[0], MONITOR_ANS_PTY, payload, sizeof(payload), &used) != 0)
        return fail("wrong-reply", -1);
    uint32_t name_length = get_u32_be(payload + 4);
    if (name_length < 1 || name_length > 128)
        return fail("wrong-name-length", (long)name_length);
    char name[128] = {0};
    memcpy(name, payload + 8, name_length);
    size_t login_offset = 8 + name_length;
    uint32_t login_length = get_u32_be(payload + login_offset);
    size_t identity = login_offset + 4 + login_length;
    if (identity + 32 > used)
        return fail("wrong-reply-size", (long)used);
    uint64_t slave_ino = get_u64_be(payload + identity + 24);

    int master = recv_fd(pair[0], "wrong-master");
    if (master < 0)
        return fail("wrong-recv-master", -1);
    int slave = recv_fd(pair[0], "wrong-slave");
    if (slave < 0)
        return fail("wrong-recv-slave", -1);

    int by_name = open(name, O_RDWR | O_NOCTTY);
    if (by_name < 0) {
        printf("pty-monitor: wrong-name=detected open_errno=%d\n", errno);
    } else {
        struct stat by_name_stat;
        if (fstat(by_name, &by_name_stat) != 0)
            return fail("wrong-fstat", -1);
        if (slave_ino != 0 && (uint64_t)by_name_stat.st_ino == slave_ino) {
            close(by_name);
            return fail("wrong-name-not-detected", (long)by_name_stat.st_ino);
        }
        printf("pty-monitor: wrong-name=detected mismatch\n");
        close(by_name);
    }

    if (send_frame(pair[0], MONITOR_REQ_PTYCLEANUP, NULL, 0) != 0)
        return fail("wrong-cleanup", -1);
    if (wait_monitor(child, 0) != 0)
        return fail("wrong-monitor-status", -1);
    close(slave);
    close(master);
    close(pair[0]);
    return 0;
}

static int run_missing_request(void) {
    int pair[2];
    if (socketpair(AF_UNIX, SOCK_STREAM, 0, pair) != 0)
        return fail("socketpair-missing", -1);
    pid_t child = fork();
    if (child < 0)
        return fail("fork-missing", -1);
    if (child == 0) {
        close(pair[0]);
        _exit(monitor_main(pair[1], MODE_SHORT_REQUEST));
    }
    close(pair[1]);

    unsigned char header[4];
    put_u32_be(header, 1);
    if (write_all(pair[0], header, sizeof(header)) != 0)
        return fail("missing-header", -1);
    if (shutdown(pair[0], SHUT_WR) != 0)
        return fail("missing-shutdown", -1);
    printf("pty-monitor: session missing-request header=1\n");

    int status = 0;
    if (waitpid(child, &status, 0) != child)
        return fail("missing-wait", -1);
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 2)
        return fail("missing-status", status);
    printf("pty-monitor: missing-request=detected status=2\n");
    close(pair[0]);
    return 0;
}

int main(void) {
    printf("pty-monitor: start uid=%d\n", (int)getuid());
    if (run_positive() != 0)
        return 1;
    if (run_drop_fd() != 0)
        return 1;
    if (run_wrong_name() != 0)
        return 1;
    if (run_missing_request() != 0)
        return 1;
    printf("pty-monitor: ok\n");
    return 0;
}
