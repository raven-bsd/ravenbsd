/* SPDX-License-Identifier: MIT
 * Copyright (c) 2005-2020 Rich Felker, et al.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: derived from musl:include/netinet/in.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
#ifndef	_NETINET_IN_H
#define	_NETINET_IN_H
#include <sys/cdefs.h>
#include <bits/abi_netinet.h>

__BEGIN_DECLS

#include <inttypes.h>
#include <sys/socket.h>

typedef uint16_t in_port_t;
typedef uint32_t in_addr_t;
struct in_addr { in_addr_t s_addr; };

struct sockaddr_in {
	sa_family_t sin_family;
	in_port_t sin_port;
	struct in_addr sin_addr;
	uint8_t sin_zero[8];
};

struct in6_addr {
	union {
		uint8_t __s6_addr[16];
		uint16_t __s6_addr16[8];
		uint32_t __s6_addr32[4];
	} __in6_union;
};
#define s6_addr __in6_union.__s6_addr
#define s6_addr16 __in6_union.__s6_addr16
#define s6_addr32 __in6_union.__s6_addr32

struct sockaddr_in6 {
	sa_family_t     sin6_family;
	in_port_t       sin6_port;
	uint32_t        sin6_flowinfo;
	struct in6_addr sin6_addr;
	uint32_t        sin6_scope_id;
};

struct ipv6_mreq {
	struct in6_addr ipv6mr_multiaddr;
	unsigned        ipv6mr_interface;
};

#define IN6ADDR_ANY_INIT      { { { 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0 } } }
#define IN6ADDR_LOOPBACK_INIT { { { 0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1 } } }

extern const struct in6_addr in6addr_any, in6addr_loopback;

#define INET_ADDRSTRLEN  16
#define INET6_ADDRSTRLEN 46

uint32_t htonl(uint32_t);
uint16_t htons(uint16_t);
uint32_t ntohl(uint32_t);
uint16_t ntohs(uint16_t);

#define IPPORT_RESERVED 1024

#define IN6_IS_ADDR_UNSPECIFIED(a) \
        (((uint32_t *) (a))[0] == 0 && ((uint32_t *) (a))[1] == 0 && \
         ((uint32_t *) (a))[2] == 0 && ((uint32_t *) (a))[3] == 0)

#define IN6_IS_ADDR_LOOPBACK(a) \
        (((uint32_t *) (a))[0] == 0 && ((uint32_t *) (a))[1] == 0 && \
         ((uint32_t *) (a))[2] == 0 && \
         ((uint8_t *) (a))[12] == 0 && ((uint8_t *) (a))[13] == 0 && \
         ((uint8_t *) (a))[14] == 0 && ((uint8_t *) (a))[15] == 1 )

#define IN6_IS_ADDR_MULTICAST(a) (((uint8_t *) (a))[0] == 0xff)

#define IN6_IS_ADDR_LINKLOCAL(a) \
        ((((uint8_t *) (a))[0]) == 0xfe && (((uint8_t *) (a))[1] & 0xc0) == 0x80)

#define IN6_IS_ADDR_SITELOCAL(a) \
        ((((uint8_t *) (a))[0]) == 0xfe && (((uint8_t *) (a))[1] & 0xc0) == 0xc0)

#define IN6_IS_ADDR_V4MAPPED(a) \
        (((uint32_t *) (a))[0] == 0 && ((uint32_t *) (a))[1] == 0 && \
         ((uint8_t *) (a))[8] == 0 && ((uint8_t *) (a))[9] == 0 && \
         ((uint8_t *) (a))[10] == 0xff && ((uint8_t *) (a))[11] == 0xff)

#define IN6_IS_ADDR_V4COMPAT(a) \
        (((uint32_t *) (a))[0] == 0 && ((uint32_t *) (a))[1] == 0 && \
         ((uint32_t *) (a))[2] == 0 && \
         !IN6_IS_ADDR_UNSPECIFIED(a) && !IN6_IS_ADDR_LOOPBACK(a))

#define IN6_IS_ADDR_MC_NODELOCAL(a) \
        (IN6_IS_ADDR_MULTICAST(a) && ((((uint8_t *) (a))[1] & 0xf) == 0x1))

#define IN6_IS_ADDR_MC_LINKLOCAL(a) \
        (IN6_IS_ADDR_MULTICAST(a) && ((((uint8_t *) (a))[1] & 0xf) == 0x2))

#define IN6_IS_ADDR_MC_SITELOCAL(a) \
        (IN6_IS_ADDR_MULTICAST(a) && ((((uint8_t *) (a))[1] & 0xf) == 0x5))

#define IN6_IS_ADDR_MC_ORGLOCAL(a) \
        (IN6_IS_ADDR_MULTICAST(a) && ((((uint8_t *) (a))[1] & 0xf) == 0x8))

#define IN6_IS_ADDR_MC_GLOBAL(a) \
        (IN6_IS_ADDR_MULTICAST(a) && ((((uint8_t *) (a))[1] & 0xf) == 0xe))

#define __ARE_4_EQUAL(a,b) \
	(!( (0[a]-0[b]) | (1[a]-1[b]) | (2[a]-2[b]) | (3[a]-3[b]) ))
#define IN6_ARE_ADDR_EQUAL(a,b) \
	__ARE_4_EQUAL((const uint32_t *)(a), (const uint32_t *)(b))

#define	IN_CLASSA(a)		((((in_addr_t)(a)) & 0x80000000) == 0)
#define	IN_CLASSA_NET		0xff000000
#define	IN_CLASSA_NSHIFT	24
#define	IN_CLASSA_HOST		(0xffffffff & ~IN_CLASSA_NET)
#define	IN_CLASSA_MAX		128
#define	IN_CLASSB(a)		((((in_addr_t)(a)) & 0xc0000000) == 0x80000000)
#define	IN_CLASSB_NET		0xffff0000
#define	IN_CLASSB_NSHIFT	16
#define	IN_CLASSB_HOST		(0xffffffff & ~IN_CLASSB_NET)
#define	IN_CLASSB_MAX		65536
#define	IN_CLASSC(a)		((((in_addr_t)(a)) & 0xe0000000) == 0xc0000000)
#define	IN_CLASSC_NET		0xffffff00
#define	IN_CLASSC_NSHIFT	8
#define	IN_CLASSC_HOST		(0xffffffff & ~IN_CLASSC_NET)
#define	IN_CLASSD(a)		((((in_addr_t)(a)) & 0xf0000000) == 0xe0000000)
#define	IN_MULTICAST(a)		IN_CLASSD(a)
#define	IN_EXPERIMENTAL(a)	((((in_addr_t)(a)) & 0xe0000000) == 0xe0000000)
#define	IN_BADCLASS(a)		((((in_addr_t)(a)) & 0xf0000000) == 0xf0000000)

#define IN_LOOPBACKNET 127

struct ip_opts {
	struct in_addr ip_dst;
	char ip_opts[40];
};

#if __BSD_VISIBLE

#define MCAST_JOIN_GROUP   42
#define MCAST_BLOCK_SOURCE 43
#define MCAST_UNBLOCK_SOURCE      44
#define MCAST_LEAVE_GROUP  45
#define MCAST_JOIN_SOURCE_GROUP   46
#define MCAST_LEAVE_SOURCE_GROUP  47
#define MCAST_MSFILTER     48

#define MCAST_EXCLUDE 0
#define MCAST_INCLUDE 1

struct ip_mreq {
	struct in_addr imr_multiaddr;
	struct in_addr imr_interface;
};

struct ip_mreqn {
	struct in_addr imr_multiaddr;
	struct in_addr imr_address;
	int imr_ifindex;
};

struct ip_mreq_source {
	struct in_addr imr_multiaddr;
	struct in_addr imr_interface;
	struct in_addr imr_sourceaddr;
};

struct ip_msfilter {
	struct in_addr imsf_multiaddr;
	struct in_addr imsf_interface;
	uint32_t imsf_fmode;
	uint32_t imsf_numsrc;
	struct in_addr imsf_slist[1];
};
#define IP_MSFILTER_SIZE(numsrc) \
	(sizeof(struct ip_msfilter) - sizeof(struct in_addr) \
	+ (numsrc) * sizeof(struct in_addr))

struct group_req {
	uint32_t gr_interface;
	struct sockaddr_storage gr_group;
};

struct group_source_req {
	uint32_t gsr_interface;
	struct sockaddr_storage gsr_group;
	struct sockaddr_storage gsr_source;
};

struct group_filter {
	uint32_t gf_interface;
	struct sockaddr_storage gf_group;
	uint32_t gf_fmode;
	uint32_t gf_numsrc;
	struct sockaddr_storage gf_slist[1];
};
#define GROUP_FILTER_SIZE(numsrc) \
	(sizeof(struct group_filter) - sizeof(struct sockaddr_storage) \
	+ (numsrc) * sizeof(struct sockaddr_storage))

struct in_pktinfo {
	int ipi_ifindex;
	struct in_addr ipi_spec_dst;
	struct in_addr ipi_addr;
};

struct in6_pktinfo {
	struct in6_addr ipi6_addr;
	unsigned ipi6_ifindex;
};

struct ip6_mtuinfo {
	struct sockaddr_in6 ip6m_addr;
	uint32_t ip6m_mtu;
};
#endif

#define __UAPI_DEF_IN_ADDR      0
#define __UAPI_DEF_IN_IPPROTO   0
#define __UAPI_DEF_IN_PKTINFO   0
#define __UAPI_DEF_IP_MREQ      0
#define __UAPI_DEF_SOCKADDR_IN  0
#define __UAPI_DEF_IN_CLASS     0
#define __UAPI_DEF_IN6_ADDR     0
#define __UAPI_DEF_IN6_ADDR_ALT 0
#define __UAPI_DEF_SOCKADDR_IN6 0
#define __UAPI_DEF_IPV6_MREQ    0
#define __UAPI_DEF_IPPROTO_V6   0
#define __UAPI_DEF_IPV6_OPTIONS 0
#define __UAPI_DEF_IN6_PKTINFO  0
#define __UAPI_DEF_IP6_MTUINFO  0

__END_DECLS

#endif
