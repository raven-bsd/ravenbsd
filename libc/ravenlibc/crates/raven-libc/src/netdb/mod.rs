// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The name-service family of the C-ABI shell (C64 4.7, rows P02/P18/P19/X40/X41/X43/X44/Z14).
//!
//! The pure codec exports (`inet_*`, `hton*`, `dn_*`, `ns_get/put`, `res_init`, `gai_strerror`,
//! `hstrerror`, `herror`, `h_errno`) go through C51's `raven-libc-resolv`. The runtime calls
//! (`getaddrinfo`, `getifaddrs`/`freeifaddrs`, `res_query`/`__res_state`, the host/service/
//! protocol/network iterators) use the merged `raven-libc-net` `Net`/`Env` seam. `res_querydomain`
//! and the `ns_initparse`/`ns_parserr` pair remain deferred.

#[path = "gethostbyname.rs"]
pub(crate) mod host;
#[path = "gethostbyaddr.rs"]
pub(crate) mod hostaddr;
#[path = "endhostent.rs"]
pub(crate) mod hostend;
pub(crate) mod msg;
#[path = "getnetbyaddr.rs"]
pub(crate) mod net;
#[path = "setnetent.rs"]
pub(crate) mod netent;
#[path = "getprotobyname.rs"]
pub(crate) mod proto;
#[path = "getprotoent.rs"]
pub(crate) mod protoent;
pub(crate) mod records;
#[path = "getaddrinfo.rs"]
pub(crate) mod runtime;
#[path = "getservbyname.rs"]
pub(crate) mod serv;
#[path = "getservent.rs"]
pub(crate) mod servent;
pub(crate) mod write;

pub(crate) mod __h_errno_location;
pub(crate) mod dn_expand;
pub(crate) mod dn_skipname;
pub(crate) mod gai_strerror;
pub(crate) mod getifaddrs;
pub(crate) mod h_errno;
pub(crate) mod herror;
pub(crate) mod hstrerror;
pub(crate) mod htonl;
pub(crate) mod htons;
pub(crate) mod if_freenameindex;
pub(crate) mod if_indextoname;
pub(crate) mod if_nameindex;
pub(crate) mod if_names;
pub(crate) mod if_nametoindex;
pub(crate) mod in6addr_any;
pub(crate) mod in6addr_loopback;
pub(crate) mod inet_addr;
pub(crate) mod inet_aton;
pub(crate) mod inet_lnaof;
pub(crate) mod inet_makeaddr;
pub(crate) mod inet_netof;
pub(crate) mod inet_network;
pub(crate) mod inet_ntoa;
pub(crate) mod inet_ntop;
pub(crate) mod inet_pton;
pub(crate) mod ns_get16;
pub(crate) mod ns_get32;
pub(crate) mod ns_put16;
pub(crate) mod ns_put32;
pub(crate) mod ns_skiprr;
pub(crate) mod ntohl;
pub(crate) mod ntohs;
pub(crate) mod res_init;
pub(crate) mod res_query;
pub(crate) mod res_state;
