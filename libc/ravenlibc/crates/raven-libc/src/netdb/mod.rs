// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The name-service family of the C-ABI shell (C64 4.7, rows P02/P18/P19/X40/X41/X43/X44/Z14).
//!
//! The pure codec exports (`inet_*`, `hton*`, `dn_*`, `ns_get/put`, `res_init`, `gai_strerror`,
//! `hstrerror`, `herror`, `h_errno`) go through C51's `raven-libc-resolv`. The calls that need
//! C63's `Net`/`Env` seam — `getaddrinfo`, the host/service/protocol/network iterators,
//! `getifaddrs`, `if_*`, `res_query`/`res_send` and `res_mkquery` (query entropy) — are
//! deferred because `raven-libc-net` is not merged on `carl/libc`; every name is in
//! `expected/DEFERRED.txt` with that reason. `ns_initparse`/`ns_parserr` are deferred too: C51's
//! `NsMsg` owns a borrowed slice and cannot be reconstructed from the pointer-bearing C
//! `ns_msg`; the pointer builders (`ns_get16`/`ns_put32`/`ns_skiprr`) are exported.

pub(crate) mod msg;
pub(crate) mod write;

pub(crate) mod __h_errno_location;
pub(crate) mod dn_expand;
pub(crate) mod dn_skipname;
pub(crate) mod gai_strerror;
pub(crate) mod h_errno;
pub(crate) mod herror;
pub(crate) mod hstrerror;
pub(crate) mod htonl;
pub(crate) mod htons;
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
