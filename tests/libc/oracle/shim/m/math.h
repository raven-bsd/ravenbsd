/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <math.h> for the raven-libc-m oracle: the host header, then the three musl
 * internal names that the host header also declares with a different prototype (`__cos`,
 * `__sin`, `__tan` are one-argument glibc internals) are renamed for the donor sources that
 * follow. The rename happens after the include, so only the donor's own definitions use it.
 */
#ifndef RL_M_MATH_H
#define RL_M_MATH_H
#include_next <math.h>
#define __cos m_cos
#define __sin m_sin
#define __tan m_tan
#define __cosl m_cosl
#define __sinl m_sinl
#define __tanl m_tanl
#endif
