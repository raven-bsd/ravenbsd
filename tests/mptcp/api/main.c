/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* MPTCP2c1: the userland IPPROTO_MPTCP API contract.
 *
 * The test creates a loopback MPTCP connection, adds a backup path, marks it backup,
 * queries the connection state, sends and receives through the pair, and checks the exact
 * errno for a wrong socket type. On the current tree the first socket() call reaches netd
 * but `net:mptcp` is not implemented yet; the qtest fails on the missing fd line and the
 * report records the errno. The expectation is not weakened while MPTCP2c2 lands. */

#include <errno.h>
#include <netinet/in.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#ifndef IPPROTO_MPTCP
#define IPPROTO_MPTCP 262
#endif
#ifndef SOL_MPTCP
#define SOL_MPTCP 284
#endif
#define MPTCP_INFO 1
#define MPTCP_SUBFLOW_ADDRS 2
#define MPTCP_PM_CMD_ADD_ADDR 1
#define MPTCP_PM_CMD_SET_FLAGS 7

struct mptcp_pm_addr {
    uint8_t family;
    uint8_t id;
    uint16_t flags;
    uint32_t addr4;
    uint16_t port;
    uint8_t pad[2];
};

struct mptcp_info {
    uint8_t state;
    uint8_t pad[3];
    uint32_t subflows;
    uint32_t active;
};

static int fail(const char *what, int value) {
    printf("mptcp-api: fail %s=%d errno=%d\n", what, value, errno);
    return 1;
}

static int make_listener(struct sockaddr_in *address) {
    socklen_t length = sizeof(*address);
    int fd = socket(AF_INET, SOCK_STREAM, IPPROTO_MPTCP);
    if (fd < 0) {
        return -1;
    }
    memset(address, 0, sizeof(*address));
    address->sin_family = AF_INET;
    address->sin_port = 0;
    address->sin_addr.s_addr = htonl(0x7f000001); /* 127.0.0.1 */
    if (bind(fd, (const struct sockaddr *)address, sizeof(*address)) != 0 ||
        listen(fd, 1) != 0 || getsockname(fd, (struct sockaddr *)address, &length) != 0) {
        (void)close(fd);
        return -1;
    }
    return fd;
}

int main(void) {
    struct sockaddr_in address;
    struct mptcp_pm_addr path;
    struct mptcp_info info;
    socklen_t info_len = sizeof(info);
    int listener;
    int client;
    int accepted;
    int bad;
    char buffer[8];

    printf("mptcp-api: start\n");

    listener = make_listener(&address);
    if (listener < 0) {
        printf("mptcp-api: listener errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: listener fd=%d port=%u\n", listener, (unsigned)ntohs(address.sin_port));

    client = socket(AF_INET, SOCK_STREAM, IPPROTO_MPTCP);
    if (client < 0) {
        printf("mptcp-api: socket errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: socket fd=%d\n", client);

    if (connect(client, (const struct sockaddr *)&address, sizeof(address)) != 0) {
        return fail("connect", -1);
    }
    printf("mptcp-api: connect=ok\n");

    accepted = accept(listener, NULL, NULL);
    if (accepted < 0) {
        return fail("accept", -1);
    }
    printf("mptcp-api: accept fd=%d\n", accepted);

    memset(&path, 0, sizeof(path));
    path.family = AF_INET;
    path.id = 1;
    path.addr4 = htonl(0x7f000001);
    path.port = 0;
    if (setsockopt(client, SOL_MPTCP, MPTCP_PM_CMD_ADD_ADDR, &path, sizeof(path)) != 0) {
        printf("mptcp-api: add-path errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: add-path id=%u\n", (unsigned)path.id);

    path.flags = 1; /* backup */
    if (setsockopt(client, SOL_MPTCP, MPTCP_PM_CMD_SET_FLAGS, &path, sizeof(path)) != 0) {
        printf("mptcp-api: backup errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: backup id=%u\n", (unsigned)path.id);

    memset(&info, 0, sizeof(info));
    if (getsockopt(client, SOL_MPTCP, MPTCP_INFO, &info, &info_len) != 0) {
        printf("mptcp-api: info errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: info state=%u subflows=%u active=%u\n", (unsigned)info.state,
           (unsigned)info.subflows, (unsigned)info.active);

    if (send(client, "hello", 5, 0) != 5) {
        return fail("send", -1);
    }
    if (recv(accepted, buffer, 5, 0) != 5 || memcmp(buffer, "hello", 5) != 0) {
        return fail("recv", -1);
    }
    printf("mptcp-api: roundtrip=5\n");

    if (close(accepted) != 0 || close(client) != 0 || close(listener) != 0) {
        return fail("close", -1);
    }
    printf("mptcp-api: close=ok\n");

    bad = socket(AF_INET, SOCK_DGRAM, IPPROTO_MPTCP);
    if (bad >= 0) {
        (void)close(bad);
        return fail("wrong-type", bad);
    }
    if (errno != EPROTONOSUPPORT && errno != ESOCKTNOSUPPORT) {
        return fail("wrong-type-errno", errno);
    }
    printf("mptcp-api: wrong-type errno=%d\n", errno);
    printf("mptcp-api: ok\n");
    return 0;
}
