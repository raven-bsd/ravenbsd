/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */

/* MPTCP2c1: the userland IPPROTO_MPTCP API contract.
 *
 * The test is deliberately first-red: it requests the MPTCP socket, adds a backup
 * path, marks it backup, queries the connection state and checks the exact errno for
 * a wrong socket type. On the current tree the first socket() call is the blocker
 * (libc validate_proto accepts only 0/6); the qtest fails on the missing fd line and
 * the report records the exact errno. */

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

int main(void) {
    struct mptcp_pm_addr path;
    struct mptcp_info info;
    socklen_t info_len = sizeof(info);
    int fd;
    int bad;

    printf("mptcp-api: start\n");

    fd = socket(AF_INET, SOCK_STREAM, IPPROTO_MPTCP);
    if (fd < 0) {
        printf("mptcp-api: socket errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: socket fd=%d\n", fd);

    memset(&path, 0, sizeof(path));
    path.family = AF_INET;
    path.id = 1;
    path.addr4 = htonl(0x0a4d0210); /* 10.77.2.16, a second local address */
    path.port = htons(0);
    if (setsockopt(fd, SOL_MPTCP, MPTCP_PM_CMD_ADD_ADDR, &path, sizeof(path)) != 0) {
        printf("mptcp-api: add-path errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: add-path id=%u\n", (unsigned)path.id);

    path.flags = 1; /* backup */
    if (setsockopt(fd, SOL_MPTCP, MPTCP_PM_CMD_SET_FLAGS, &path, sizeof(path)) != 0) {
        printf("mptcp-api: backup errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: backup id=%u\n", (unsigned)path.id);

    memset(&info, 0, sizeof(info));
    if (getsockopt(fd, SOL_MPTCP, MPTCP_INFO, &info, &info_len) != 0) {
        printf("mptcp-api: info errno=%d\n", errno);
        return 1;
    }
    printf("mptcp-api: info state=%u subflows=%u active=%u\n", (unsigned)info.state,
           (unsigned)info.subflows, (unsigned)info.active);

    if (send(fd, "hello", 5, 0) != 5) {
        return fail("send", -1);
    }
    printf("mptcp-api: send=5\n");
    if (close(fd) != 0) {
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
