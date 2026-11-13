/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/netinet/ip6.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _NETINET_IP6_H
#define _NETINET_IP6_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#include <stdint.h>
#include <netinet/in.h>

struct ip6_hdr {
	union {
		struct ip6_hdrctl {
			uint32_t ip6_un1_flow;
			uint16_t ip6_un1_plen;
			uint8_t  ip6_un1_nxt;
			uint8_t  ip6_un1_hlim;
		} ip6_un1;
		uint8_t ip6_un2_vfc;
	} ip6_ctlun;
	struct in6_addr ip6_src;
	struct in6_addr ip6_dst;
};

#define ip6_vfc   ip6_ctlun.ip6_un2_vfc
#define ip6_flow  ip6_ctlun.ip6_un1.ip6_un1_flow
#define ip6_plen  ip6_ctlun.ip6_un1.ip6_un1_plen
#define ip6_nxt   ip6_ctlun.ip6_un1.ip6_un1_nxt
#define ip6_hlim  ip6_ctlun.ip6_un1.ip6_un1_hlim
#define ip6_hops  ip6_ctlun.ip6_un1.ip6_un1_hlim

struct ip6_ext {
	uint8_t  ip6e_nxt;
	uint8_t  ip6e_len;
};

struct ip6_hbh {
	uint8_t  ip6h_nxt;
	uint8_t  ip6h_len;
};

struct ip6_dest {
	uint8_t  ip6d_nxt;
	uint8_t  ip6d_len;
};

struct ip6_rthdr {
	uint8_t  ip6r_nxt;
	uint8_t  ip6r_len;
	uint8_t  ip6r_type;
	uint8_t  ip6r_segleft;
};

struct ip6_rthdr0 {
	uint8_t  ip6r0_nxt;
	uint8_t  ip6r0_len;
	uint8_t  ip6r0_type;
	uint8_t  ip6r0_segleft;
	uint8_t  ip6r0_reserved;
	uint8_t  ip6r0_slmap[3];
	struct in6_addr ip6r0_addr[];
};

struct ip6_frag {
	uint8_t   ip6f_nxt;
	uint8_t   ip6f_reserved;
	uint16_t  ip6f_offlg;
	uint32_t  ip6f_ident;
};

#define IP6F_OFF_MASK       0xf8ff
#define IP6F_RESERVED_MASK  0x0600
#define IP6F_MORE_FRAG      0x0100

struct ip6_opt {
	uint8_t  ip6o_type;
	uint8_t  ip6o_len;
};

#define IP6OPT_TYPE(o)		((o) & 0xc0)
#define IP6OPT_TYPE_SKIP	0x00
#define IP6OPT_TYPE_DISCARD	0x40
#define IP6OPT_TYPE_FORCEICMP	0x80
#define IP6OPT_TYPE_ICMP	0xc0
#define IP6OPT_TYPE_MUTABLE	0x20

#define IP6OPT_PAD1	0
#define IP6OPT_PADN	1

#define IP6OPT_JUMBO		0xc2
#define IP6OPT_NSAP_ADDR	0xc3
#define IP6OPT_TUNNEL_LIMIT	0x04
#define IP6OPT_ROUTER_ALERT	0x05

struct ip6_opt_jumbo {
	uint8_t  ip6oj_type;
	uint8_t  ip6oj_len;
	uint8_t  ip6oj_jumbo_len[4];
};
#define IP6OPT_JUMBO_LEN	6

struct ip6_opt_nsap {
	uint8_t  ip6on_type;
	uint8_t  ip6on_len;
	uint8_t  ip6on_src_nsap_len;
	uint8_t  ip6on_dst_nsap_len;
};

struct ip6_opt_tunnel {
	uint8_t  ip6ot_type;
	uint8_t  ip6ot_len;
	uint8_t  ip6ot_encap_limit;
};

struct ip6_opt_router {
	uint8_t  ip6or_type;
	uint8_t  ip6or_len;
	uint8_t  ip6or_value[2];
};

#define IP6_ALERT_MLD	0x0000
#define IP6_ALERT_RSVP	0x0100
#define IP6_ALERT_AN	0x0200

__END_DECLS

#endif
