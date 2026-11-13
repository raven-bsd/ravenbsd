/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Replacement "libc.h" for the musl stdio oracle, written from scratch: the two global flags
 * the stdio files read, with no locale or TLS machinery.
 */
#ifndef RL_STDIO_MUSL_LIBC_H
#define RL_STDIO_MUSL_LIBC_H

#include <stdlib.h>
#include <stdio.h>
#include <limits.h>

struct __libc {
	char can_do_threads;
	char threaded;
	char secure;
	volatile signed char need_locks;
	int threads_minus_1;
};

static struct __libc __rl_libc = { 0, 1, 0, 1, 0 };
#define libc __rl_libc

static inline char *__rl_randname(char *template)
{
	static const char alphabet[] = "abcdefghijklmnopqrstuvwxyz0123456789";
	static unsigned counter;
	size_t i;
	for (i = 0; i < 6; i++)
		template[i] = alphabet[(counter + i) % (sizeof alphabet - 1)];
	counter++;
	return template;
}
#define __randname(s) __rl_randname(s)

#endif
