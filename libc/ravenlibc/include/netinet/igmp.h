/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/netinet/igmp.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
 * (no per-file header upstream; covered by musl/COPYRIGHT; public header, notice kept under R-H6)
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef _NETINET_IGMP_H
#define _NETINET_IGMP_H
#include <sys/cdefs.h>

#include <stdint.h>
#include <netinet/in.h>

struct igmp {
	uint8_t igmp_type;
	uint8_t igmp_code;
	uint16_t igmp_cksum;
	struct in_addr igmp_group;
};

#define IGMP_MINLEN			8

#define IGMP_MEMBERSHIP_QUERY   	0x11
#define IGMP_V1_MEMBERSHIP_REPORT	0x12
#define IGMP_V2_MEMBERSHIP_REPORT	0x16
#define IGMP_V2_LEAVE_GROUP		0x17

#define IGMP_DVMRP			0x13
#define IGMP_PIM			0x14
#define IGMP_TRACE			0x15

#define IGMP_MTRACE_RESP		0x1e
#define IGMP_MTRACE			0x1f

#define IGMP_MAX_HOST_REPORT_DELAY	10
#define IGMP_TIMER_SCALE		10

#define IGMP_DELAYING_MEMBER	1
#define IGMP_IDLE_MEMBER	2
#define IGMP_LAZY_MEMBER	3
#define IGMP_SLEEPING_MEMBER	4
#define IGMP_AWAKENING_MEMBER	5

#define IGMP_v1_ROUTER		1
#define IGMP_v2_ROUTER		2

#define IGMP_HOST_MEMBERSHIP_QUERY	IGMP_MEMBERSHIP_QUERY
#define IGMP_HOST_MEMBERSHIP_REPORT	IGMP_V1_MEMBERSHIP_REPORT
#define IGMP_HOST_NEW_MEMBERSHIP_REPORT	IGMP_V2_MEMBERSHIP_REPORT
#define IGMP_HOST_LEAVE_MESSAGE		IGMP_V2_LEAVE_GROUP

#endif
