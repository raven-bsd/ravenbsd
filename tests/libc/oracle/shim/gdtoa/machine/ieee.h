/* SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 * RavenBSD oracle shim: the x86_64 IEEE layouts used by hdtoa.c and ldtoa.c. Written from the
 * amd64 psABI (little-endian, 80-bit x87 extended with six tail bytes of padding), not copied
 * from the NetBSD header.
 */
#ifndef _ORACLE_GDTOA_MACHINE_IEEE_H
#define _ORACLE_GDTOA_MACHINE_IEEE_H

#include <stdint.h>

#define DBL_EXPBITS 11
#define DBL_FRACHBITS 20
#define DBL_FRACLBITS 32
#define DBL_FRACBITS (DBL_FRACLBITS + DBL_FRACHBITS)

struct ieee_double {
	uint32_t dbl_fracl : 32;
	uint32_t dbl_frach : 20;
	uint32_t dbl_exp : 11;
	uint32_t dbl_sign : 1;
};

union ieee_double_u {
	double dblu_d;
	struct ieee_double dblu_dbl;
};

#define EXT_EXPBITS 15
#define EXT_FRACHBITS 32
#define EXT_FRACLBITS 32
#define EXT_FRACBITS (EXT_FRACLBITS + EXT_FRACHBITS)

struct ieee_ext {
	uint32_t ext_fracl : 32;
	uint32_t ext_frach : 32;
	uint32_t ext_exp : 15;
	uint32_t ext_sign : 1;
};

union ieee_ext_u {
	long double extu_ld;
	struct ieee_ext extu_ext;
};

#define EXT_TO_ARRAY32(u, a) do {					\
	(a)[0] = (uint32_t)(u).extu_ext.ext_fracl;			\
	(a)[1] = (uint32_t)(u).extu_ext.ext_frach;			\
} while (/*CONSTCOND*/ 0)

#endif
