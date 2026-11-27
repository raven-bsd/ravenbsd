/*-
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Copyright (c) 1982, 1986, 1993
 *	The Regents of the University of California.  All rights reserved.
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * RavenBSD: public interface records ported from FreeBSD
 * sys/net/if.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-3-Clause).
 *
 * Scope (S745, S700 D-3, PG-42): the interface records and flags are the pinned FreeBSD
 * definitions. Interface discovery is provided by getifaddrs(3)/if_nameindex(3); the
 * SIOCGIF* ioctls are classified but not implemented by this milestone (ENOSYS), and
 * routing sockets are unsupported. Variable-length statistics are reported in struct
 * if_data only when a provider supplies them.
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

#ifndef _NET_IF_H_
#define _NET_IF_H_

#include <sys/cdefs.h>

__BEGIN_DECLS

#include <stdint.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <sys/time.h>

/*
 * Length of interface external name, including terminating '\0'.
 */
#define	IF_NAMESIZE	16
#define	IFNAMSIZ	IF_NAMESIZE

#if __BSD_VISIBLE
#define	IF_MAXUNIT	0x7fff	/* historical value */

/*
 * Structure used to query names of interface cloners.
 */
struct if_clonereq {
	int	ifcr_total;		/* total cloners (out) */
	int	ifcr_count;		/* room for this many in user buffer */
	char	*ifcr_buffer;		/* buffer for cloner names */
};

/*
 * Structure describing information about an interface
 * which may be of interest to management entities.
 */
struct if_data {
	/* generic interface information */
	uint8_t	ifi_type;		/* ethernet, tokenring, etc */
	uint8_t	ifi_physical;		/* e.g., AUI, Thinnet, 10base-T, etc */
	uint8_t	ifi_addrlen;		/* media address length */
	uint8_t	ifi_hdrlen;		/* media header length */
	uint8_t	ifi_link_state;		/* current link state */
	uint8_t	ifi_vhid;		/* carp vhid */
	uint16_t	ifi_datalen;	/* length of this data struct */
	uint32_t	ifi_mtu;	/* maximum transmission unit */
	uint32_t	ifi_metric;	/* routing metric (external only) */
	uint64_t	ifi_baudrate;	/* linespeed */
	/* volatile statistics */
	uint64_t	ifi_ipackets;	/* packets received on interface */
	uint64_t	ifi_ierrors;	/* input errors on interface */
	uint64_t	ifi_opackets;	/* packets sent on interface */
	uint64_t	ifi_oerrors;	/* output errors on interface */
	uint64_t	ifi_collisions;	/* collisions on csma interfaces */
	uint64_t	ifi_ibytes;	/* total number of octets received */
	uint64_t	ifi_obytes;	/* total number of octets sent */
	uint64_t	ifi_imcasts;	/* packets received via multicast */
	uint64_t	ifi_omcasts;	/* packets sent via multicast */
	uint64_t	ifi_iqdrops;	/* dropped on input */
	uint64_t	ifi_oqdrops;	/* dropped on output */
	uint64_t	ifi_noproto;	/* destined for unsupported protocol */
	uint64_t	ifi_hwassist;	/* HW offload capabilities, see IFCAP */

	/* Unions are here to make sizes MI. */
	union {				/* uptime at attach or stat reset */
		time_t		tt;
		uint64_t	ph;
	} __ifi_epoch;
#define	ifi_epoch	__ifi_epoch.tt
	union {				/* time of last administrative change */
		struct timeval	tv;
		struct {
			uint64_t ph1;
			uint64_t ph2;
		} ph;
	} __ifi_lastchange;
#define	ifi_lastchange	__ifi_lastchange.tv
};

/* Interface flags. */
#define	IFF_UP		0x1		/* interface is up */
#define	IFF_BROADCAST	0x2		/* broadcast address valid */
#define	IFF_DEBUG	0x4		/* turn on debugging */
#define	IFF_LOOPBACK	0x8		/* is a loopback net */
#define	IFF_POINTOPOINT	0x10		/* is a point-to-point link */
#define	IFF_NEEDSEPOCH	0x20		/* calls if_input w/o net epoch */
#define	IFF_DRV_RUNNING	0x40		/* resources allocated */
#define	IFF_NOARP	0x80		/* no address resolution protocol */
#define	IFF_PROMISC	0x100		/* receive all packets */
#define	IFF_ALLMULTI	0x200		/* receive all multicast packets */
#define	IFF_DRV_OACTIVE	0x400		/* tx hardware queue is full */
#define	IFF_SIMPLEX	0x800		/* can't hear own transmissions */
#define	IFF_LINK0	0x1000		/* per link layer defined bit */
#define	IFF_LINK1	0x2000		/* per link layer defined bit */
#define	IFF_LINK2	0x4000		/* per link layer defined bit */
#define	IFF_ALTPHYS	IFF_LINK2	/* use alternate physical connection */
#define	IFF_MULTICAST	0x8000		/* supports multicast */
#define	IFF_CANTCONFIG	0x10000		/* unconfigurable using ioctl(2) */
#define	IFF_PPROMISC	0x20000		/* user-requested promisc mode */
#define	IFF_MONITOR	0x40000		/* user-requested monitor mode */
#define	IFF_STATICARP	0x80000		/* static ARP */
#define	IFF_STICKYARP	0x100000	/* sticky ARP */
#define	IFF_DYING	0x200000	/* interface is winding down */
#define	IFF_PALLMULTI	0x800000	/* user-requested allmulti mode */
#define	IFF_NETLINK_1	0x1000000	/* used by netlink */

/* Backward compatibility. */
#define	IFF_RUNNING	IFF_DRV_RUNNING
#define	IFF_OACTIVE	IFF_DRV_OACTIVE

#define	IFF_CANTCHANGE	(IFF_BROADCAST | IFF_SIMPLEX | IFF_MULTICAST |	\
	IFF_ALLMULTI | IFF_NEEDSEPOCH)

/*
 * Message format for use in obtaining information about interfaces
 * from getkerninfo and the routing socket.
 */
struct if_msghdr {
	unsigned short	ifm_msglen;	/* to skip over non-understood messages */
	unsigned char	ifm_version;	/* future binary compatibility */
	unsigned char	ifm_type;	/* message type */
	int	ifm_addrs;	/* like rtm_addrs */
	int	ifm_flags;	/* value of if_flags */
	unsigned short	ifm_index;	/* index for associated ifp */
	unsigned short _ifm_spare1;
	struct	if_data ifm_data;/* statistics and other data about if */
};

#define	IF_MSGHDRL_IFM_DATA(_l) \
    (struct if_data *)((char *)(_l) + (_l)->ifm_data_off)
#define	IF_MSGHDRL_RTA(_l) \
    (void *)((uintptr_t)(_l) + (_l)->ifm_len)
struct if_msghdrl {
	unsigned short	ifm_msglen;	/* to skip over non-understood messages */
	unsigned char	ifm_version;	/* future binary compatibility */
	unsigned char	ifm_type;	/* message type */
	int	ifm_addrs;	/* like rtm_addrs */
	int	ifm_flags;	/* value of if_flags */
	unsigned short	ifm_index;	/* index for associated ifp */
	unsigned short _ifm_spare1;	/* spare space to grow if_index */
	unsigned short	ifm_len;	/* length of if_msghdrl incl. if_data */
	unsigned short	ifm_data_off;	/* offset of if_data from beginning */
	int	_ifm_spare2;
	struct	if_data ifm_data;/* statistics and other data about if */
};

/*
 * Message format for use in obtaining information about interface addresses
 * from getkerninfo and the routing socket.
 */
struct ifa_msghdr {
	unsigned short	ifam_msglen;	/* to skip over non-understood messages */
	unsigned char	ifam_version;	/* future binary compatibility */
	unsigned char	ifam_type;	/* message type */
	int	ifam_addrs;	/* like rtm_addrs */
	int	ifam_flags;	/* value of ifa_flags */
	unsigned short	ifam_index;	/* index for associated ifp */
	unsigned short _ifam_spare1;
	int	ifam_metric;	/* value of ifa_ifp->if_metric */
};

#define	IFA_MSGHDRL_IFAM_DATA(_l) \
    (struct if_data *)((char *)(_l) + (_l)->ifam_data_off)
#define	IFA_MSGHDRL_RTA(_l) \
    (void *)((uintptr_t)(_l) + (_l)->ifam_len)
struct ifa_msghdrl {
	unsigned short	ifam_msglen;	/* to skip over non-understood messages */
	unsigned char	ifam_version;	/* future binary compatibility */
	unsigned char	ifam_type;	/* message type */
	int	ifam_addrs;	/* like rtm_addrs */
	int	ifam_flags;	/* value of ifa_flags */
	unsigned short	ifam_index;	/* index for associated ifp */
	unsigned short _ifam_spare1;	/* spare space to grow if_index */
	unsigned short	ifam_len;	/* length of ifa_msghdrl incl. if_data */
	unsigned short	ifam_data_off;	/* offset of if_data from beginning */
	int	ifam_metric;	/* value of ifa_ifp->if_metric */
	struct	if_data ifam_data;/* statistics and other data about if or address */
};

/*
 * Message format for use in obtaining information about multicast addresses
 * from the routing socket.
 */
struct ifma_msghdr {
	unsigned short	ifmam_msglen;	/* to skip over non-understood messages */
	unsigned char	ifmam_version;	/* future binary compatibility */
	unsigned char	ifmam_type;	/* message type */
	int	ifmam_addrs;	/* like rtm_addrs */
	int	ifmam_flags;	/* value of ifa_flags */
	unsigned short	ifmam_index;	/* index for associated ifp */
	unsigned short _ifmam_spare1;
};

/*
 * Message format announcing the arrival or departure of a network interface.
 */
struct if_announcemsghdr {
	unsigned short	ifan_msglen;	/* to skip over non-understood messages */
	unsigned char	ifan_version;	/* future binary compatibility */
	unsigned char	ifan_type;	/* message type */
	unsigned short	ifan_index;	/* index for associated ifp */
	char	ifan_name[IFNAMSIZ]; /* if name, e.g. "en0" */
	unsigned short	ifan_what;	/* what type of announcement */
};

#define	IFAN_ARRIVAL	0	/* interface arrival */
#define	IFAN_DEPARTURE	1	/* interface departure */

/*
 * Buffer with length to be used in SIOCGIFDESCR/SIOCSIFDESCR requests.
 */
struct ifreq_buffer {
	size_t	length;
	void	*buffer;
};

struct ifreq_nv_req {
	unsigned int	buf_length;	/* Total size of buffer, unsigned int for ABI struct ifreq */
	unsigned int	length;		/* Length of the filled part */
	void	*buffer;	/* Buffer itself, containing packed nv */
};

#define	IFR_CAP_NV_MAXBUFSIZE	(2 * 1024 * 1024)

/*
 * Interface request structure used for socket ioctl's.  All interface ioctl's
 * must have parameter definitions which begin with ifr_name.
 */
struct ifreq {
	char	ifr_name[IFNAMSIZ];		/* if name, e.g. "en0" */
	union {
		struct	sockaddr ifru_addr;
		struct	sockaddr ifru_dstaddr;
		struct	sockaddr ifru_broadaddr;
		struct	ifreq_buffer ifru_buffer;
		short	ifru_flags[2];
		short	ifru_index;
		int	ifru_jid;
		int	ifru_metric;
		int	ifru_mtu;
		int	ifru_phys;
		int	ifru_media;
		caddr_t	ifru_data;
		int	ifru_cap[2];
		unsigned int	ifru_fib;
		unsigned char	ifru_vlan_pcp;
		struct	ifreq_nv_req ifru_nv;
	} ifr_ifru;
#define	ifr_addr	ifr_ifru.ifru_addr	/* address */
#define	ifr_dstaddr	ifr_ifru.ifru_dstaddr	/* other end of p-to-p link */
#define	ifr_broadaddr	ifr_ifru.ifru_broadaddr	/* broadcast address */
#define	ifr_buffer	ifr_ifru.ifru_buffer	/* user supplied buffer with its length */
#define	ifr_flags	ifr_ifru.ifru_flags[0]	/* flags (low 16 bits) */
#define	ifr_flagshigh	ifr_ifru.ifru_flags[1]	/* flags (high 16 bits) */
#define	ifr_jid		ifr_ifru.ifru_jid	/* jail/vnet */
#define	ifr_metric	ifr_ifru.ifru_metric	/* metric */
#define	ifr_mtu		ifr_ifru.ifru_mtu	/* mtu */
#define ifr_phys	ifr_ifru.ifru_phys	/* physical wire */
#define ifr_media	ifr_ifru.ifru_media	/* physical media */
#define	ifr_data	ifr_ifru.ifru_data	/* for use by interface */
#define	ifr_reqcap	ifr_ifru.ifru_cap[0]	/* requested capabilities */
#define	ifr_curcap	ifr_ifru.ifru_cap[1]	/* current capabilities */
#define	ifr_index	ifr_ifru.ifru_index	/* interface index */
#define	ifr_fib		ifr_ifru.ifru_fib	/* interface fib */
#define	ifr_vlan_pcp	ifr_ifru.ifru_vlan_pcp	/* VLAN priority */
#define	ifr_lan_pcp	ifr_ifru.ifru_vlan_pcp	/* VLAN priority */
#define	ifr_cap_nv	ifr_ifru.ifru_nv	/* nv-based cap interface */
};

#define	_SIZEOF_ADDR_IFREQ(ifr) \
	((ifr).ifr_addr.sa_len > sizeof(struct sockaddr) ? \
	 (sizeof(struct ifreq) - sizeof(struct sockaddr) + \
	  (ifr).ifr_addr.sa_len) : sizeof(struct ifreq))

struct ifaliasreq {
	char	ifra_name[IFNAMSIZ];		/* if name, e.g. "en0" */
	struct	sockaddr ifra_addr;
	struct	sockaddr ifra_broadaddr;
	struct	sockaddr ifra_mask;
	int	ifra_vhid;
};

/* 9.x compat */
struct oifaliasreq {
	char	ifra_name[IFNAMSIZ];
	struct	sockaddr ifra_addr;
	struct	sockaddr ifra_broadaddr;
	struct	sockaddr ifra_mask;
};

struct ifmediareq {
	char	ifm_name[IFNAMSIZ];	/* if name, e.g. "en0" */
	int	ifm_current;		/* current media options */
	int	ifm_mask;		/* don't care mask */
	int	ifm_status;		/* media status */
	int	ifm_active;		/* active options */
	int	ifm_count;		/* # entries in ifm_ulist array */
	int	*ifm_ulist;		/* media words */
};

struct ifdrv {
	char		ifd_name[IFNAMSIZ];	/* if name, e.g. "en0" */
	unsigned long	ifd_cmd;
	size_t		ifd_len;
	void		*ifd_data;
};

#define	IFSTATMAX	800		/* 10 lines of text */
struct ifstat {
	char	ifs_name[IFNAMSIZ];	/* if name, e.g. "en0" */
	char	ascii[IFSTATMAX + 1];
};

/*
 * Structure used in SIOCGIFCONF request.
 */
struct ifconf {
	int	ifc_len;		/* size of associated buffer */
	union {
		caddr_t	ifcu_buf;
		struct	ifreq *ifcu_req;
	} ifc_ifcu;
#define	ifc_buf	ifc_ifcu.ifcu_buf	/* buffer address */
#define	ifc_req	ifc_ifcu.ifcu_req	/* array of structures returned */
};

/* Interface groups. */
#define	IFG_ALL		"all"		/* group contains all interfaces */
#define	IFG_EGRESS	"egress"	/* if(s) default route(s) point to */

struct ifg_req {
	union {
		char			 ifgrqu_group[IFNAMSIZ];
		char			 ifgrqu_member[IFNAMSIZ];
	} ifgrq_ifgrqu;
#define	ifgrq_group	ifgrq_ifgrqu.ifgrqu_group
#define	ifgrq_member	ifgrq_ifgrqu.ifgrqu_member
};

struct ifgroupreq {
	char	ifgr_name[IFNAMSIZ];
	unsigned int	ifgr_len;
	union {
		char	ifgru_group[IFNAMSIZ];
		struct	ifg_req *ifgru_groups;
	} ifgr_ifgru;
#define ifgr_group	ifgr_ifgru.ifgru_group
#define ifgr_groups	ifgr_ifgru.ifgru_groups
};
#endif /* __BSD_VISIBLE */

struct if_nameindex {
	unsigned int	if_index;	/* 1, 2, ... */
	char		*if_name;	/* null terminated name: "le0", ... */
};

void			 if_freenameindex(struct if_nameindex *);
char			*if_indextoname(unsigned int, char *);
struct if_nameindex	*if_nameindex(void);
unsigned int		 if_nametoindex(const char *);

__END_DECLS

#endif /* _NET_IF_H_ */
