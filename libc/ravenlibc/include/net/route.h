/*-
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 1980, 1986, 1993
 *	The Regents of the University of California.  All rights reserved.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: public routing records ported from FreeBSD
 * sys/net/route.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-3-Clause).
 *
 * Scope (S745, S700 D-3, R31/O-abi-6, PG-42): routing sockets are not implemented for
 * this milestone. The records below are the pinned FreeBSD compile-time definitions;
 * no route provider exists and socket(AF_ROUTE, ...) fails with EAFNOSUPPORT. Do not
 * treat the presence of these declarations as routing support.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */

#ifndef _NET_ROUTE_H_
#define _NET_ROUTE_H_

#include <sys/cdefs.h>

__BEGIN_DECLS

#include <sys/types.h>
#include <sys/socket.h>

struct rt_metrics {
	unsigned long	rmx_locks;	/* Kernel must leave these values alone */
	unsigned long	rmx_mtu;	/* MTU for this path */
	unsigned long	rmx_hopcount;	/* max hops expected */
	unsigned long	rmx_expire;	/* lifetime for route, e.g. redirect */
	unsigned long	rmx_recvpipe;	/* inbound delay-bandwidth product */
	unsigned long	rmx_sendpipe;	/* outbound delay-bandwidth product */
	unsigned long	rmx_ssthresh;	/* outbound gateway buffer limit */
	unsigned long	rmx_rtt;	/* estimated round trip time */
	unsigned long	rmx_rttvar;	/* estimated rtt variance */
	unsigned long	rmx_pksent;	/* packets sent using this route */
	unsigned long	rmx_weight;	/* route weight */
	unsigned long	rmx_nhidx;	/* route nexhop index */
	unsigned long	rmx_metric;	/* route metric */
	unsigned long	rmx_filler[1];
};

#define	RTM_RTTUNIT	1000000	/* units for rtt, rttvar, as units per sec */
#define	rmx_state	rmx_weight

#define	RT_DEFAULT_WEIGHT	1
#define	RT_DEFAULT_METRIC	1
#define	RT_WILDCARD_METRIC	0

#define	RT_DEFAULT_FIB	0	/* Explicitly mark fib=0 restricted cases */
#define	RT_ALL_FIBS	-1	/* Announce event for every fib */

#define	RTF_UP		0x1		/* route usable */
#define	RTF_GATEWAY	0x2		/* destination is a gateway */
#define	RTF_HOST	0x4		/* host entry (net otherwise) */
#define	RTF_REJECT	0x8		/* host or net unreachable */
#define	RTF_DYNAMIC	0x10		/* created dynamically (by redirect) */
#define	RTF_MODIFIED	0x20		/* modified dynamically (by redirect) */
#define RTF_DONE	0x40		/* message confirmed */
#define RTF_XRESOLVE	0x200		/* external daemon resolves name */
#define RTF_LLINFO	0x400		/* DEPRECATED - exists ONLY for backward compatibility */
#define RTF_LLDATA	0x400		/* used by apps to add/del L2 entries */
#define RTF_STATIC	0x800		/* manually added */
#define RTF_BLACKHOLE	0x1000		/* just discard pkts (during updates) */
#define RTF_PROTO2	0x4000		/* protocol specific routing flag */
#define RTF_PROTO1	0x8000		/* protocol specific routing flag */
#define RTF_PROTO3	0x40000		/* protocol specific routing flag */
#define	RTF_FIXEDMTU	0x80000		/* MTU was explicitly specified */
#define RTF_PINNED	0x100000	/* route is immutable */
#define	RTF_LOCAL	0x200000	/* route represents a local address */
#define	RTF_BROADCAST	0x400000	/* route represents a bcast address */
#define	RTF_MULTICAST	0x800000	/* route represents a mcast address */
#define	RTF_STICKY	0x10000000	/* always route dst->src */
#define	RTF_GWFLAG_COMPAT 0x80000000	/* compatibility bit for existing routing apps */

#define RTF_FMASK	\
	(RTF_PROTO1 | RTF_PROTO2 | RTF_PROTO3 | RTF_BLACKHOLE | \
	 RTF_REJECT | RTF_STATIC | RTF_STICKY)

/* Routing message types. */
#define	RTM_ADD		0x1	/* Add Route */
#define	RTM_DELETE	0x2	/* Delete Route */
#define	RTM_CHANGE	0x3	/* Change Metrics or flags */
#define	RTM_GET		0x4	/* Report Metrics */
#define	RTM_LOSING	0x5	/* Kernel Suspects Partitioning */
#define	RTM_REDIRECT	0x6	/* Told to use different route */
#define	RTM_MISS	0x7	/* Lookup failed on this address */
#define	RTM_LOCK	0x8	/* fix specified metrics */
#define	RTM_RESOLVE	0xb	/* req to resolve dst to LL addr */
#define	RTM_NEWADDR	0xc	/* address being added to iface */
#define	RTM_DELADDR	0xd	/* address being removed from iface */
#define	RTM_IFINFO	0xe	/* iface going up/down etc. */
#define	RTM_NEWMADDR	0xf	/* mcast group membership being added to if */
#define	RTM_DELMADDR	0x10	/* mcast group membership being deleted */
#define	RTM_IFANNOUNCE	0x11	/* iface arrival/departure */
#define	RTM_IEEE80211	0x12	/* IEEE80211 wireless event */
#define	RTM_IPFWLOG	0x13	/* IPFW rule match log event */

#define RTM_VERSION	5	/* Up the ante and ignore older versions */

/* Bitmask values for rtm_inits and rmx_locks. */
#define RTV_MTU		0x1	/* init or lock _mtu */
#define RTV_HOPCOUNT	0x2	/* init or lock _hopcount */
#define RTV_EXPIRE	0x4	/* init or lock _expire */
#define RTV_RPIPE	0x8	/* init or lock _recvpipe */
#define RTV_SPIPE	0x10	/* init or lock _sendpipe */
#define RTV_SSTHRESH	0x20	/* init or lock _ssthresh */
#define RTV_RTT		0x40	/* init or lock _rtt */
#define RTV_RTTVAR	0x80	/* init or lock _rttvar */
#define RTV_WEIGHT	0x100	/* init or lock _weight */
#define RTV_METRIC	0x200	/* init or lock _metric */

/* Bitmask values for rtm_addrs. */
#define RTA_DST		0x1	/* destination sockaddr present */
#define RTA_GATEWAY	0x2	/* gateway sockaddr present */
#define RTA_NETMASK	0x4	/* netmask sockaddr present */
#define RTA_GENMASK	0x8	/* cloning mask sockaddr present */
#define RTA_IFP		0x10	/* interface name sockaddr present */
#define RTA_IFA		0x20	/* interface addr sockaddr present */
#define RTA_AUTHOR	0x40	/* sockaddr for author of redirect */
#define RTA_BRD		0x80	/* for NEWADDR, broadcast or p-p dest addr */

/* Index offsets for sockaddr array for alternate internal encoding. */
#define RTAX_DST	0	/* destination sockaddr present */
#define RTAX_GATEWAY	1	/* gateway sockaddr present */
#define RTAX_NETMASK	2	/* netmask sockaddr present */
#define RTAX_GENMASK	3	/* cloning mask sockaddr present */
#define RTAX_IFP	4	/* interface name sockaddr present */
#define RTAX_IFA	5	/* interface addr sockaddr present */
#define RTAX_AUTHOR	6	/* sockaddr for author of redirect */
#define RTAX_BRD	7	/* for NEWADDR, broadcast or p-p dest addr */
#define RTAX_MAX	8	/* size of array to allocate */

struct rtentry;
struct nhop_object;
struct ifaddr;
struct ifnet;
typedef int rib_filter_f_t(const struct rtentry *, const struct nhop_object *, void *);

struct rt_addrinfo {
	int	rti_addrs;			/* Route RTF_ flags */
	int	rti_flags;			/* Route RTF_ flags */
	struct	sockaddr *rti_info[RTAX_MAX];	/* Sockaddr data */
	struct	ifaddr *rti_ifa;		/* value of rt_ifa addr */
	struct	ifnet *rti_ifp;			/* route interface */
	rib_filter_f_t	*rti_filter;		/* filter function */
	void	*rti_filterdata;		/* filter parameters */
	unsigned long	rti_mflags;			/* metrics RTV_ flags */
	unsigned long	rti_spare;			/* Will be used for fib */
	struct	rt_metrics *rti_rmx;		/* Pointer to route metrics */
};

/*
 * Structures for routing messages.
 */
struct rt_msghdr {
	unsigned short	rtm_msglen;	/* to skip over non-understood messages */
	unsigned char	rtm_version;	/* future binary compatibility */
	unsigned char	rtm_type;	/* message type */
	unsigned short	rtm_index;	/* index for associated ifp */
	unsigned short _rtm_spare1;
	int	rtm_flags;	/* flags, incl. kern & message, e.g. DONE */
	int	rtm_addrs;	/* bitmask identifying sockaddrs in msg */
	pid_t	rtm_pid;	/* identify sender */
	int	rtm_seq;	/* for sender to identify action */
	int	rtm_errno;	/* why failed */
	int	rtm_fmask;	/* bitmask used in RTM_CHANGE message */
	unsigned long	rtm_inits;	/* which metrics we are initializing */
	struct	rt_metrics rtm_rmx; /* metrics themselves */
};

/*
 * This macro returns the size of a struct sockaddr when passed
 * through a routing socket. Basically we round up sa_len to
 * a multiple of sizeof(long), with a minimum of sizeof(long).
 */
#define SA_SIZE(sa)						\
    (  (((struct sockaddr *)(sa))->sa_len == 0) ?		\
	sizeof(long)		:				\
	1 + ( (((struct sockaddr *)(sa))->sa_len - 1) | (sizeof(long) - 1) ) )

__END_DECLS

#endif
