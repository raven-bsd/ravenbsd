/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Compile-only FreeBSD application-profile fixture (DR-69, S721).
 *
 * Expected values come from the pinned FreeBSD donor
 * 133344405f532bd54ff3629a77d4c4e062f01fdb:
 *   sys/sys/errno.h        EAGAIN 35
 *   sys/sys/signal.h       SIGUSR1 30
 *   sys/sys/_types.h       typedef __uint8_t sa_family_t
 *   sys/sys/socket.h       struct sockaddr_in, sin_family at offset 1
 * Run it against the Raven public headers; every failure names the profile
 * gap that the FreeBSD cutover tickets must close.
 */

#include <errno.h>
#include <signal.h>
#include <stddef.h>
#include <sys/socket.h>
#include <netinet/in.h>

_Static_assert(EAGAIN == 35, "FreeBSD errno: EAGAIN is 35");
_Static_assert(SIGUSR1 == 30, "FreeBSD signal: SIGUSR1 is 30");
_Static_assert(sizeof(sa_family_t) == 1, "FreeBSD sa_family_t is one byte");
_Static_assert(
    offsetof(struct sockaddr_in, sin_family) == 1,
    "FreeBSD sockaddr_in.sin_family is at offset 1 after sin_len");
_Static_assert(sizeof(struct sockaddr_in) == 16, "FreeBSD sockaddr_in is 16 bytes");
_Static_assert(sizeof(struct sockaddr) == 16, "FreeBSD sockaddr is 16 bytes");
_Static_assert(
    offsetof(struct sockaddr, sa_len) == 0,
    "FreeBSD sockaddr.sa_len is the first byte");
_Static_assert(
    offsetof(struct sockaddr, sa_family) == 1,
    "FreeBSD sockaddr.sa_family follows sa_len");
