/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/* RavenBSD: the C65 compat-crypt shims (arc4random, explicit_memset, _PASSWORD_LEN). */
#ifndef C65_BCRYPT_SHIM_H
#define C65_BCRYPT_SHIM_H
#include <stdint.h>
#include <stddef.h>
#ifndef _PASSWORD_LEN
#define _PASSWORD_LEN 128
#endif
static inline void _c65_explicit_memset(void *p, int c, size_t n)
{
	volatile unsigned char *v = p;
	while (n--)
		*v++ = (unsigned char)c;
}
#define explicit_memset _c65_explicit_memset
static inline uint32_t _c65_arc4random(void)
{
	static uint32_t state = 0x12345678u;
	state ^= state << 13;
	state ^= state >> 17;
	state ^= state << 5;
	return state;
}
#define arc4random _c65_arc4random
#endif
