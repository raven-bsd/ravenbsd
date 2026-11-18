// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Minimal original `h_macros.h` for the NetBSD ATF tests: the small assertion and garbage-buffer
// helpers the donation uses, expressed only over the vendored `atf-c` API and the target libc.
// This is not a copy of the NetBSD header.

#ifndef RL_NETBSD_H_MACROS_H
#define RL_NETBSD_H_MACROS_H

#include <sys/types.h>

#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <atf-c.h>

#define REQUIRE_LIBC(x, v) \
    ATF_REQUIRE_MSG((x) != (v), "%s: %s", #x, strerror(errno))
#define CHECK_LIBC(x, v) \
    ATF_CHECK_MSG((x) != (v), "%s: %s", #x, strerror(errno))
#define RL(x) REQUIRE_LIBC(x, -1)
#define RLF(x, fmt, arg) \
    ATF_CHECK_MSG((x) != -1, "%s [" fmt "]: %s", #x, arg, strerror(errno))
#define RZ(x)                                                              \
    do {                                                                    \
        int rl_rz_value = (x);                                              \
        ATF_REQUIRE_MSG(rl_rz_value == 0, "%s: %s", #x,                   \
                        strerror(rl_rz_value));                             \
    } while (0)

static inline void atf_tc_fail_errno(const char *fmt, ...) {
    char buffer[1024];
    int saved = errno;
    va_list ap;
    va_start(ap, fmt);
    (void)vsnprintf(buffer, sizeof(buffer), fmt, ap);
    va_end(ap);
    (void)strlcat(buffer, ": ", sizeof(buffer));
    (void)strlcat(buffer, strerror(saved), sizeof(buffer));
    atf_tc_fail("%s", buffer);
}

static inline void tests_makegarbage(void *space, size_t length) {
    uint16_t *slots = space;
    uint16_t value;
    while (length >= sizeof(value)) {
        *slots++ = (uint16_t)random();
        length -= sizeof(value);
    }
    value = (uint16_t)random();
    memcpy(slots, &value, length);
}

#endif /* RL_NETBSD_H_MACROS_H */
