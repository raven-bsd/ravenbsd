/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/netinet/udp.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _NETINET_UDP_H
#define _NETINET_UDP_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#include <stdint.h>

#if __BSD_VISIBLE
#define uh_sport source
#define uh_dport dest
#define uh_ulen len
#define uh_sum check
#endif

struct udphdr {
	uint16_t uh_sport;
	uint16_t uh_dport;
	uint16_t uh_ulen;
	uint16_t uh_sum;
};

#define UDP_CORK	1
#define UDP_ENCAP	100
#define UDP_NO_CHECK6_TX 101
#define UDP_NO_CHECK6_RX 102
#define UDP_SEGMENT	103
#define UDP_GRO		104

#define UDP_ENCAP_ESPINUDP_NON_IKE 1
#define UDP_ENCAP_ESPINUDP	2
#define UDP_ENCAP_L2TPINUDP	3
#define UDP_ENCAP_GTP0		4
#define UDP_ENCAP_GTP1U		5
#define UDP_ENCAP_RXRPC		6
#define TCP_ENCAP_ESPINTCP	7

#define SOL_UDP            17

__END_DECLS

#endif
