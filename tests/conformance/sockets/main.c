// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S174 socket conformance probe. One-shot guest service; prints CONFORM rows for:
// sockaddr layout/byte order, SO_REUSEADDR, accept/getpeername, SO_ERROR once,
// shutdown(SHUT_WR) FIN + read-after-EOF, the half-close FIN/EOF/HUP row,
// shutdown(SHUT_RD), shutdown(SHUT_RDWR), and listener close/rebind.

#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#define LISTEN_PORT 23100

static int failures;

static int fail(const char *row, const char *why)
{
    printf("CONFORM %s FAIL %s\n", row, why);
    failures++;
    return -1;
}

static int pass(const char *row, const char *detail)
{
    printf("CONFORM %s PASS %s\n", row, detail);
    return 0;
}

static int read_exact(int fd, void *buffer, size_t length)
{
    unsigned char *bytes = buffer;
    size_t done = 0;
    while (done < length) {
        ssize_t got = read(fd, bytes + done, length - done);
        if (got < 0 && errno == EINTR)
            continue;
        if (got <= 0)
            return -1;
        done += (size_t)got;
    }
    return 0;
}

static int write_exact(int fd, const void *buffer, size_t length)
{
    const unsigned char *bytes = buffer;
    size_t done = 0;
    while (done < length) {
        ssize_t wrote = write(fd, bytes + done, length - done);
        if (wrote < 0 && errno == EINTR)
            continue;
        if (wrote <= 0)
            return -1;
        done += (size_t)wrote;
    }
    return 0;
}

static int listener_stage_errno;
static const char *listener_stage = "none";

static int listener(void)
{
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address;
    int one = 1;
    if (fd < 0) {
        listener_stage = "socket";
        listener_stage_errno = errno;
        return -1;
    }
    if (setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one)) != 0) {
        listener_stage = "setsockopt";
        listener_stage_errno = errno;
        close(fd);
        return -1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_len = sizeof(address);
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons(LISTEN_PORT);
    if (bind(fd, (struct sockaddr *)&address, sizeof(address)) != 0) {
        listener_stage = "bind";
        listener_stage_errno = errno;
        close(fd);
        return -1;
    }
    if (listen(fd, 8) != 0) {
        listener_stage = "listen";
        listener_stage_errno = errno;
        close(fd);
        return -1;
    }
    return fd;
}

static int accept_one(int listen_fd)
{
    struct pollfd pfd;
    pfd.fd = listen_fd;
    pfd.events = POLLIN;
    pfd.revents = 0;
    if (poll(&pfd, 1, 30000) <= 0)
        return -1;
    return accept(listen_fd, NULL, NULL);
}

static int row_layout(void)
{
    struct sockaddr_in in;
    struct sockaddr_in6 in6;
    struct sockaddr_un un;
    struct sockaddr_storage storage;
    struct msghdr msg;
    struct cmsghdr cmsg;
    struct addrinfo ai;
    unsigned short port = htons(0x1234);
    unsigned char wire[2];
    memcpy(wire, &port, sizeof(wire));
    /* Literal FreeBSD records at the S700 pin: the length byte is first and the family
     * is one byte at offset 1. These are independent expectations, not a Raven-to-Raven
     * comparison. */
    if (sizeof(in) != 16 || offsetof(struct sockaddr_in, sin_len) != 0 ||
        offsetof(struct sockaddr_in, sin_family) != 1 ||
        offsetof(struct sockaddr_in, sin_port) != 2 ||
        offsetof(struct sockaddr_in, sin_addr) != 4 ||
        sizeof(in6) != 28 || offsetof(struct sockaddr_in6, sin6_len) != 0 ||
        offsetof(struct sockaddr_in6, sin6_family) != 1 ||
        offsetof(struct sockaddr_un, sun_len) != 0 ||
        offsetof(struct sockaddr_un, sun_family) != 1 ||
        offsetof(struct sockaddr_un, sun_path) != 2 || sizeof(un) != 106 ||
        offsetof(struct sockaddr_storage, ss_len) != 0 ||
        offsetof(struct sockaddr_storage, ss_family) != 1 ||
        sizeof(storage) != 128 || sizeof(msg) != 48 ||
        offsetof(struct msghdr, msg_flags) != 44 || sizeof(cmsg) != 12 ||
        offsetof(struct cmsghdr, cmsg_level) != 4 ||
        offsetof(struct cmsghdr, cmsg_type) != 8 ||
        offsetof(struct addrinfo, ai_canonname) != 24 ||
        offsetof(struct addrinfo, ai_addr) != 32 || AF_INET6 != 28 ||
        wire[0] != 0x12 || wire[1] != 0x34) {
        char detail[160];
        (void)snprintf(detail, sizeof(detail),
                       "in=%u in6=%u un=%u storage=%u msg=%u cmsg=%u af6=%d "
                       "port=%02x%02x",
                       (unsigned)sizeof(in), (unsigned)sizeof(in6),
                       (unsigned)sizeof(un), (unsigned)sizeof(storage),
                       (unsigned)sizeof(msg), (unsigned)sizeof(cmsg), AF_INET6,
                       wire[0], wire[1]);
        return fail("layout", detail);
    }
    return pass("layout",
                "freebsd sa_len=0 family=1 in=16 in6=28 un=106 storage=128 "
                "msg=48 cmsg=12 af6=28 be-port");
}

static int row_reuseaddr(int listen_fd)
{
    int value = 0;
    socklen_t value_len = sizeof(value);
    if (getsockopt(listen_fd, SOL_SOCKET, SO_REUSEADDR, &value, &value_len) != 0)
        return fail("reuseaddr", "getsockopt SO_REUSEADDR");
    if (value != 1) {
        char detail[48];
        (void)snprintf(detail, sizeof(detail), "SO_REUSEADDR=%d", value);
        return fail("reuseaddr", detail);
    }
    return pass("reuseaddr", "getsockopt=1");
}

static int row_accept(int listen_fd)
{
    struct sockaddr_in peer;
    socklen_t peer_len = sizeof(peer);
    unsigned char port[5];
    const unsigned char *ip;
    int fd = accept_one(listen_fd);
    if (fd < 0)
        return fail("accept", "accept timeout");
    if (read_exact(fd, port, sizeof(port)) != 0) {
        close(fd);
        return fail("accept", "peer port read");
    }
    {
        const unsigned char ack = 0x5a;
        if (write_exact(fd, &ack, 1) != 0) {
            close(fd);
            return fail("accept", "ack write");
        }
    }
    if (getpeername(fd, (struct sockaddr *)&peer, &peer_len) != 0) {
        close(fd);
        return fail("accept", "getpeername");
    }
    ip = (const unsigned char *)&peer.sin_addr;
    if (port[0] != 1 || peer.sin_family != AF_INET || ip[0] != 10 ||
        ip[1] != 0 || ip[2] != 2 || ip[3] != 2 ||
        memcmp(&peer.sin_port, &port[1], 2) != 0) {
        char detail[96];
        unsigned peer_port = ntohs(peer.sin_port);
        unsigned wire_port = ((unsigned)port[1] << 8) | port[2];
        (void)snprintf(detail, sizeof(detail),
                       "id=%u peer=%u.%u.%u.%u:%u wire=%u", port[0], ip[0],
                       ip[1], ip[2], ip[3], peer_port, wire_port);
        close(fd);
        return fail("accept", detail);
    }
    close(fd);
    return pass("accept", "peer=10.0.2.2 exact port");
}

static int row_soerror(void)
{
    struct sockaddr_in peer;
    struct pollfd pfd;
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    int flags;
    int error = 0;
    socklen_t error_len = sizeof(error);
    if (fd < 0)
        return fail("soerror", "socket");
    flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) != 0) {
        close(fd);
        return fail("soerror", "nonblock");
    }
    memset(&peer, 0, sizeof(peer));
    peer.sin_len = sizeof(peer);
    peer.sin_family = AF_INET;
    peer.sin_addr.s_addr = htonl(0x7f000001); /* the guest's own loopback */
    peer.sin_port = htons(9);
    if (connect(fd, (struct sockaddr *)&peer, sizeof(peer)) == 0) {
        close(fd);
        return fail("soerror", "closed port connected");
    }
    if (errno != EINPROGRESS && errno != EAGAIN) {
        close(fd);
        return fail("soerror", "connect not in progress");
    }
    pfd.fd = fd;
    pfd.events = POLLOUT;
    for (int attempt = 0; attempt < 100; attempt++) {
        pfd.revents = 0;
        if (poll(&pfd, 1, 50) < 0 && errno == EINTR)
            continue;
        error_len = sizeof(error);
        if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &error_len) != 0) {
            close(fd);
            return fail("soerror", "getsockopt");
        }
        if (error != 0)
            break;
    }
    if (error != ECONNREFUSED) {
        char detail[64];
        (void)snprintf(detail, sizeof(detail), "first SO_ERROR=%d errno=%d",
                       error, errno);
        close(fd);
        return fail("soerror", detail);
    }
    error = -1;
    error_len = sizeof(error);
    if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &error_len) != 0 ||
        error != 0) {
        close(fd);
        return fail("soerror", "second SO_ERROR not cleared");
    }
    close(fd);
    return pass("soerror", "ECONNREFUSED once then 0");
}

static int row_shutdown(int listen_fd)
{
    char word[5];
    char pong[4];
    int fd = accept_one(listen_fd);
    if (fd < 0)
        return fail("shutdown", "accept timeout");
    if (read_exact(fd, word, sizeof(word)) != 0 || word[0] != 3 ||
        memcmp(&word[1], "PING", 4) != 0) {
        close(fd);
        return fail("shutdown", "PING");
    }
    if (shutdown(fd, SHUT_WR) != 0) {
        close(fd);
        return fail("shutdown", "shutdown WR");
    }
    /* The host reads EOF from our FIN, then sends PONG; our read side is still open. */
    if (read_exact(fd, pong, sizeof(pong)) != 0 || memcmp(pong, "PONG", 4) != 0) {
        close(fd);
        return fail("shutdown", "PONG after our FIN");
    }
    close(fd);
    return pass("shutdown", "FIN then read-after-EOF");
}

static int row_half(int listen_fd)
{
    char word[5];
    char fin;
    struct pollfd pfd;
    int fd = accept_one(listen_fd);
    if (fd < 0)
        return fail("half", "accept timeout");
    if (read_exact(fd, word, sizeof(word)) != 0 || word[0] != 6 ||
        memcmp(&word[1], "DATA", 4) != 0) {
        close(fd);
        return fail("half", "DATA");
    }
    /* The peer half-closed its write side: the guest reads EOF and can still write. */
    if (read(fd, &fin, 1) != 0) {
        close(fd);
        return fail("half", "peer FIN read");
    }
    if (write_exact(fd, "HALFACK", 7) != 0) {
        close(fd);
        return fail("half", "HALFACK");
    }
    /* Guest-initiated half-close: the queued HALFACK drains and SHUT_WR emits the FIN. */
    if (shutdown(fd, SHUT_WR) != 0) {
        close(fd);
        return fail("half", "shutdown WR");
    }
    if (read(fd, &fin, 1) != 0) {
        close(fd);
        return fail("half", "EOF after our FIN");
    }
    errno = 0;
    if (write(fd, &fin, 1) >= 0 || errno != EPIPE) {
        char detail[48];
        (void)snprintf(detail, sizeof(detail), "write errno=%d", errno);
        close(fd);
        return fail("half", detail);
    }
    pfd.fd = fd;
    pfd.events = POLLIN | POLLHUP;
    pfd.revents = 0;
    if (poll(&pfd, 1, 5000) <= 0 || (pfd.revents & POLLIN) == 0) {
        close(fd);
        return fail("half", "EOF not readable after our FIN");
    }
    close(fd);
    return pass("half", "EOF, FIN, EPIPE");
}

static int row_shutrd(int listen_fd)
{
    char word[5];
    char ack[5] = "RDACK";
    char byte;
    int fd = accept_one(listen_fd);
    if (fd < 0)
        return fail("shutrd", "accept timeout");
    if (read_exact(fd, word, sizeof(word)) != 0 || word[0] != 7 ||
        memcmp(&word[1], "RDAT", 4) != 0) {
        close(fd);
        return fail("shutrd", "RDAT");
    }
    if (shutdown(fd, SHUT_RD) != 0) {
        close(fd);
        return fail("shutrd", "shutdown RD");
    }
    if (read(fd, &byte, 1) != 0) {
        close(fd);
        return fail("shutrd", "read after SHUT_RD");
    }
    if (write_exact(fd, ack, sizeof(ack)) != 0) {
        close(fd);
        return fail("shutrd", "write after SHUT_RD");
    }
    close(fd);
    return pass("shutrd", "read EOF, write still works");
}

static int row_shutrdwr(int listen_fd)
{
    char word[5];
    char byte;
    int fd = accept_one(listen_fd);
    if (fd < 0)
        return fail("shutrdwr", "accept timeout");
    if (read_exact(fd, word, sizeof(word)) != 0 || word[0] != 8 ||
        memcmp(&word[1], "RDWR", 4) != 0) {
        close(fd);
        return fail("shutrdwr", "RDWR");
    }
    if (shutdown(fd, SHUT_RDWR) != 0) {
        close(fd);
        return fail("shutrdwr", "shutdown RDWR");
    }
    if (read(fd, &byte, 1) != 0) {
        close(fd);
        return fail("shutrdwr", "read after SHUT_RDWR");
    }
    errno = 0;
    if (write(fd, &byte, 1) >= 0 || errno != EPIPE) {
        char detail[48];
        (void)snprintf(detail, sizeof(detail), "write errno=%d", errno);
        close(fd);
        return fail("shutrdwr", detail);
    }
    close(fd);
    return pass("shutrdwr", "read EOF, write EPIPE");
}

int main(void)
{
    int listen_fd;
    int total = 10;
    setvbuf(stdout, NULL, _IOLBF, 0);
    (void)signal(SIGPIPE, SIG_IGN);
    row_layout();
    listen_fd = listener();
    if (listen_fd < 0) {
        printf("CONFORM SUMMARY FAIL 0/%d\n", total);
        return 1;
    }
    row_reuseaddr(listen_fd);
    row_accept(listen_fd);
    row_soerror();
    row_shutdown(listen_fd);
    /* The half-close row uses the original listener before the close/rebind row. */
    row_half(listen_fd);
    row_shutrd(listen_fd);
    row_shutrdwr(listen_fd);
    close(listen_fd);
    /* The last accepted connection was closed by the guest; its FIN exchange may still be
     * completing in the kernel. Wait up to 1 s for the port to become reusable, then assert it. */
    {
        int attempts = 0;
        do {
            listen_fd = listener();
            if (listen_fd >= 0)
                break;
            (void)poll(NULL, 0, 10);
            attempts++;
        } while (attempts < 100);
        if (listen_fd < 0) {
            char detail[80];
            (void)snprintf(detail, sizeof(detail), "%s errno=%d after %d ms",
                           listener_stage, listener_stage_errno, attempts * 10);
            fail("rebound", detail);
        } else {
            char detail[64];
            (void)snprintf(detail, sizeof(detail), "same port after close (%d ms)",
                           attempts * 10);
            pass("rebound", detail);
        }
    }
    if (listen_fd >= 0)
        close(listen_fd);
    if (failures == 0) {
        printf("CONFORM SUMMARY PASS %d/%d\n", total, total);
        return 0;
    }
    printf("CONFORM SUMMARY FAIL %d/%d\n", total - failures, total);
    return 1;
}
