/*-
 * Copyright (c) 2025-2026 Robert Clausecker <fuz@FreeBSD.org>
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * RavenBSD: derived from freebsd:include/stdbit.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
 * (the C23 `_Generic` macros; no Rust code is generated from this file). The donor's size_t and
 * least-integer typedef block is dropped because the API uses only the five unsigned widths.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
#ifndef __STDC_VERSION_STDBIT_H__
#define __STDC_VERSION_STDBIT_H__ 202311L

#include <sys/cdefs.h>
#include <stdbool.h>

/* Byte order, C23 7.18.4. */
#define __STDC_ENDIAN_LITTLE__ __ORDER_LITTLE_ENDIAN__
#define __STDC_ENDIAN_BIG__ __ORDER_BIG_ENDIAN__
#define __STDC_ENDIAN_NATIVE__ __BYTE_ORDER__

__BEGIN_DECLS

unsigned int stdc_leading_zeros_uc(unsigned char) __pure2;
unsigned int stdc_leading_zeros_us(unsigned short) __pure2;
unsigned int stdc_leading_zeros_ui(unsigned int) __pure2;
unsigned int stdc_leading_zeros_ul(unsigned long) __pure2;
unsigned int stdc_leading_zeros_ull(unsigned long long) __pure2;
#define stdc_leading_zeros(x) __generic_bitfunc(stdc_leading_zeros, x)

unsigned int stdc_leading_ones_uc(unsigned char) __pure2;
unsigned int stdc_leading_ones_us(unsigned short) __pure2;
unsigned int stdc_leading_ones_ui(unsigned int) __pure2;
unsigned int stdc_leading_ones_ul(unsigned long) __pure2;
unsigned int stdc_leading_ones_ull(unsigned long long) __pure2;
#define stdc_leading_ones(x) __generic_bitfunc(stdc_leading_ones, x)

unsigned int stdc_trailing_zeros_uc(unsigned char) __pure2;
unsigned int stdc_trailing_zeros_us(unsigned short) __pure2;
unsigned int stdc_trailing_zeros_ui(unsigned int) __pure2;
unsigned int stdc_trailing_zeros_ul(unsigned long) __pure2;
unsigned int stdc_trailing_zeros_ull(unsigned long long) __pure2;
#define stdc_trailing_zeros(x) __generic_bitfunc(stdc_trailing_zeros, x)

unsigned int stdc_trailing_ones_uc(unsigned char) __pure2;
unsigned int stdc_trailing_ones_us(unsigned short) __pure2;
unsigned int stdc_trailing_ones_ui(unsigned int) __pure2;
unsigned int stdc_trailing_ones_ul(unsigned long) __pure2;
unsigned int stdc_trailing_ones_ull(unsigned long long) __pure2;
#define stdc_trailing_ones(x) __generic_bitfunc(stdc_trailing_ones, x)

unsigned int stdc_first_leading_zero_uc(unsigned char) __pure2;
unsigned int stdc_first_leading_zero_us(unsigned short) __pure2;
unsigned int stdc_first_leading_zero_ui(unsigned int) __pure2;
unsigned int stdc_first_leading_zero_ul(unsigned long) __pure2;
unsigned int stdc_first_leading_zero_ull(unsigned long long) __pure2;
#define stdc_first_leading_zero(x) __generic_bitfunc(stdc_first_leading_zero, x)

unsigned int stdc_first_leading_one_uc(unsigned char) __pure2;
unsigned int stdc_first_leading_one_us(unsigned short) __pure2;
unsigned int stdc_first_leading_one_ui(unsigned int) __pure2;
unsigned int stdc_first_leading_one_ul(unsigned long) __pure2;
unsigned int stdc_first_leading_one_ull(unsigned long long) __pure2;
#define stdc_first_leading_one(x) __generic_bitfunc(stdc_first_leading_one, x)

unsigned int stdc_first_trailing_zero_uc(unsigned char) __pure2;
unsigned int stdc_first_trailing_zero_us(unsigned short) __pure2;
unsigned int stdc_first_trailing_zero_ui(unsigned int) __pure2;
unsigned int stdc_first_trailing_zero_ul(unsigned long) __pure2;
unsigned int stdc_first_trailing_zero_ull(unsigned long long) __pure2;
#define stdc_first_trailing_zero(x) __generic_bitfunc(stdc_first_trailing_zero, x)

unsigned int stdc_first_trailing_one_uc(unsigned char) __pure2;
unsigned int stdc_first_trailing_one_us(unsigned short) __pure2;
unsigned int stdc_first_trailing_one_ui(unsigned int) __pure2;
unsigned int stdc_first_trailing_one_ul(unsigned long) __pure2;
unsigned int stdc_first_trailing_one_ull(unsigned long long) __pure2;
#define stdc_first_trailing_one(x) __generic_bitfunc(stdc_first_trailing_one, x)

unsigned int stdc_count_zeros_uc(unsigned char) __pure2;
unsigned int stdc_count_zeros_us(unsigned short) __pure2;
unsigned int stdc_count_zeros_ui(unsigned int) __pure2;
unsigned int stdc_count_zeros_ul(unsigned long) __pure2;
unsigned int stdc_count_zeros_ull(unsigned long long) __pure2;
#define stdc_count_zeros(x) __generic_bitfunc(stdc_count_zeros, x)

unsigned int stdc_count_ones_uc(unsigned char) __pure2;
unsigned int stdc_count_ones_us(unsigned short) __pure2;
unsigned int stdc_count_ones_ui(unsigned int) __pure2;
unsigned int stdc_count_ones_ul(unsigned long) __pure2;
unsigned int stdc_count_ones_ull(unsigned long long) __pure2;
#define stdc_count_ones(x) __generic_bitfunc(stdc_count_ones, x)

bool stdc_has_single_bit_uc(unsigned char) __pure2;
bool stdc_has_single_bit_us(unsigned short) __pure2;
bool stdc_has_single_bit_ui(unsigned int) __pure2;
bool stdc_has_single_bit_ul(unsigned long) __pure2;
bool stdc_has_single_bit_ull(unsigned long long) __pure2;
#define stdc_has_single_bit(x) __generic_bitfunc(stdc_has_single_bit, x)

unsigned int stdc_bit_width_uc(unsigned char) __pure2;
unsigned int stdc_bit_width_us(unsigned short) __pure2;
unsigned int stdc_bit_width_ui(unsigned int) __pure2;
unsigned int stdc_bit_width_ul(unsigned long) __pure2;
unsigned int stdc_bit_width_ull(unsigned long long) __pure2;
#define stdc_bit_width(x) __generic_bitfunc(stdc_bit_width, x)

unsigned char stdc_bit_floor_uc(unsigned char) __pure2;
unsigned short stdc_bit_floor_us(unsigned short) __pure2;
unsigned int stdc_bit_floor_ui(unsigned int) __pure2;
unsigned long stdc_bit_floor_ul(unsigned long) __pure2;
unsigned long long stdc_bit_floor_ull(unsigned long long) __pure2;
#define stdc_bit_floor(x) __generic_bitfunc(stdc_bit_floor, x)

unsigned char stdc_bit_ceil_uc(unsigned char) __pure2;
unsigned short stdc_bit_ceil_us(unsigned short) __pure2;
unsigned int stdc_bit_ceil_ui(unsigned int) __pure2;
unsigned long stdc_bit_ceil_ul(unsigned long) __pure2;
unsigned long long stdc_bit_ceil_ull(unsigned long long) __pure2;
#define stdc_bit_ceil(x) __generic_bitfunc(stdc_bit_ceil, x)

#ifndef __cplusplus
#define __generic_bitfunc(func, x) (_Generic((x),				\
	unsigned char: func ## _uc,					\
	unsigned short: func ## _us,					\
	unsigned int: func ## _ui,					\
	unsigned long: func ## _ul,					\
	unsigned long long: func ## _ull)(x))
#endif

__END_DECLS

#endif /* __STDC_VERSION_STDBIT_H__ */
