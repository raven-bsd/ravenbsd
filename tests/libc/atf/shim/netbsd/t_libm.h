// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Minimal original `t_libm.h` for the NetBSD libm ATF tests. The checks compare a function
// result against an expected value with a relative epsilon and report through `atf-c`.

#ifndef RL_NETBSD_T_LIBM_H
#define RL_NETBSD_T_LIBM_H

#include <math.h>

#include <atf-c.h>

#define T_LIBM_CHECK(subtest, fn, arg, expect_, epsilon_)                   \
    do {                                                                    \
        long double rl_epsilon = (epsilon_);                               \
        long double rl_expect = (expect_);                                 \
        long double rl_result = (fn)(arg);                                 \
        long double rl_error = fabsl((rl_result - rl_expect) / rl_expect); \
        if (!(rl_result == rl_expect || rl_error <= rl_epsilon)) {         \
            atf_tc_fail_nonfatal(                                          \
                "subtest %u: " #fn "(%g) is %Lg (%.14La) not %Lg "        \
                "(%.13La), error %Lg (%.6La) > %Lg",                      \
                subtest, arg, rl_result, rl_result, rl_expect, rl_expect, \
                rl_error, rl_error, rl_epsilon);                          \
        }                                                                   \
    } while (0)

#define T_LIBM_CHECK_NAN(subtest, fn, arg)                                  \
    do {                                                                    \
        double rl_result = (fn)(arg);                                       \
        if (!isnan(rl_result)) {                                            \
            atf_tc_fail_nonfatal("subtest %u: " #fn "(%g) is %g not NaN",  \
                                 subtest, arg, rl_result);                  \
        }                                                                   \
    } while (0)

#define T_LIBM_CHECK_PLUS_ZERO(subtest, fn, arg)                            \
    do {                                                                    \
        double rl_result = (fn)(arg);                                       \
        if (fabs(rl_result) > 0.0 || signbit(rl_result) != 0) {            \
            atf_tc_fail_nonfatal("subtest %u: " #fn "(%g) is %g not +0.0", \
                                 subtest, arg, rl_result);                  \
        }                                                                   \
    } while (0)

#define T_LIBM_CHECK_MINUS_ZERO(subtest, fn, arg)                           \
    do {                                                                    \
        double rl_result = (fn)(arg);                                       \
        if (fabs(rl_result) > 0.0 || signbit(rl_result) == 0) {            \
            atf_tc_fail_nonfatal("subtest %u: " #fn "(%g) is %g not -0.0", \
                                 subtest, arg, rl_result);                  \
        }                                                                   \
    } while (0)

#define T_LIBM_NAN (0.0 / 0.0)
#define T_LIBM_PLUS_INF (+1.0 / 0.0)
#define T_LIBM_MINUS_INF (-1.0 / 0.0)

#define ATF_LIBM_TEST(name, description)                                   \
    ATF_TC(name);                                                           \
    ATF_TC_HEAD(name, tc) { atf_tc_set_md_var(tc, "descr", description); } \
    ATF_TC_BODY(name, tc)

#endif /* RL_NETBSD_T_LIBM_H */
