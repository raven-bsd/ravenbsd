// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// S174 socket conformance probe. One-shot guest service; prints CONFORM rows for:
// accept/getpeername, SO_ERROR once, shutdown(SHUT_WR) FIN + read-after-EOF,
// listener close/rebind, and the half-close HUP row.

#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
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

static int listener(void)
{
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address;
    int one = 1;
    if (fd < 0)
        return -1;
    if (setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one)) != 0) {
        close(fd);
        return -1;
    }
    memset(&address, 0, sizeof(address));
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    address.sin_port = htons(LISTEN_PORT);
    if (bind(fd, (struct sockaddr *)&address, sizeof(address)) != 0 ||
        listen(fd, 8) != 0) {
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
    if (read(fd, &fin, 1) != 0) {
        close(fd);
        return fail("half", "peer FIN read");
    }
    if (write_exact(fd, "HALFACK", 7) != 0) {
        close(fd);
        return fail("half", "HALFACK");
    }
    pfd.fd = fd;
    pfd.events = POLLIN;
    for (int attempt = 0; attempt < 150; attempt++) {
        pfd.revents = 0;
        if (poll(&pfd, 1, 20) > 0 && (pfd.revents & POLLHUP) != 0) {
            close(fd);
            return pass("half", "POLLHUP after peer RST");
        }
    }
    close(fd);
    return fail("half", "no POLLHUP");
}

int main(void)
{
    int listen_fd;
    int total = 6;
    setvbuf(stdout, NULL, _IOLBF, 0);
    listen_fd = listener();
    if (listen_fd < 0) {
        printf("CONFORM SUMMARY FAIL 0/%d\n", total);
        return 1;
    }
    row_accept(listen_fd);
    row_soerror();
    row_shutdown(listen_fd);
    close(listen_fd);
    listen_fd = listener();
    if (listen_fd < 0)
        fail("rebound", "listener rebind");
    else
        pass("rebound", "same port after close");
    row_half(listen_fd);
    if (listen_fd >= 0)
        close(listen_fd);
    if (failures == 0) {
        printf("CONFORM SUMMARY PASS %d/%d\n", total, total);
        return 0;
    }
    printf("CONFORM SUMMARY FAIL %d/%d\n", total - failures, total);
    return 1;
}
