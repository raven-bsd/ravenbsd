/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/net/ethernet.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef _NET_ETHERNET_H
#define _NET_ETHERNET_H
#include <sys/cdefs.h>

__BEGIN_DECLS

#include <stdint.h>
#include <sys/types.h>
#include <netinet/if_ether.h>

struct ether_addr {
	uint8_t ether_addr_octet[ETH_ALEN];
};

struct ether_header {
	uint8_t  ether_dhost[ETH_ALEN];
	uint8_t  ether_shost[ETH_ALEN];
	uint16_t ether_type;
};

#define	ETHERTYPE_PUP		0x0200
#define ETHERTYPE_SPRITE	0x0500
#define	ETHERTYPE_IP		0x0800
#define	ETHERTYPE_ARP		0x0806
#define	ETHERTYPE_REVARP	0x8035
#define ETHERTYPE_AT		0x809B
#define ETHERTYPE_AARP		0x80F3
#define	ETHERTYPE_VLAN		0x8100
#define ETHERTYPE_IPX		0x8137
#define	ETHERTYPE_IPV6		0x86dd
#define ETHERTYPE_LOOPBACK	0x9000

#define	ETHER_ADDR_LEN	ETH_ALEN
#define	ETHER_TYPE_LEN	2
#define	ETHER_CRC_LEN	4
#define	ETHER_HDR_LEN	ETH_HLEN
#define	ETHER_MIN_LEN	(ETH_ZLEN + ETHER_CRC_LEN)
#define	ETHER_MAX_LEN	(ETH_FRAME_LEN + ETHER_CRC_LEN)

#define	ETHER_IS_VALID_LEN(foo)	\
	((foo) >= ETHER_MIN_LEN && (foo) <= ETHER_MAX_LEN)

#define	ETHERTYPE_TRAIL		0x1000
#define	ETHERTYPE_NTRAILER	16

#define	ETHERMTU	ETH_DATA_LEN
#define	ETHERMIN	(ETHER_MIN_LEN - ETHER_HDR_LEN - ETHER_CRC_LEN)

__END_DECLS

#endif
