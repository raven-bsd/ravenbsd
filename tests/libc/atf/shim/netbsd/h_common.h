// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Minimal original `h_common.h` for the NetBSD pthread ATF tests.

#ifndef RL_NETBSD_H_COMMON_H
#define RL_NETBSD_H_COMMON_H

#include <string.h>

#define PTHREAD_REQUIRE(x)                                                 \
    do {                                                                    \
        int rl_pthread_value = (x);                                        \
        ATF_REQUIRE_MSG(rl_pthread_value == 0, "%s: %s", #x,              \
                        strerror(rl_pthread_value));                        \
    } while (0)

#define PTHREAD_REQUIRE_STATUS(x, v)                                       \
    do {                                                                    \
        int rl_pthread_value = (x);                                        \
        ATF_REQUIRE_MSG(rl_pthread_value == (v), "%s: %s", #x,            \
                        strerror(rl_pthread_value));                        \
    } while (0)

#endif /* RL_NETBSD_H_COMMON_H */
