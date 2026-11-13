/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement <printf.h> for the FreeBSD printf oracle, written from scratch.
 * The xprintf registration machinery is deleted from the Rust engine and is
 * compiled out here (__use_xprintf stays 0), so the declarations are not used.
 */
#ifndef RL_FMT_PRINTF_H
#define RL_FMT_PRINTF_H

struct printf_info;
typedef int printf_arginfo_function(const struct printf_info *, size_t, int *);
typedef int printf_function(FILE *, const struct printf_info *, const void **);

int register_printf_function(int, printf_function *, printf_arginfo_function *);

#endif
