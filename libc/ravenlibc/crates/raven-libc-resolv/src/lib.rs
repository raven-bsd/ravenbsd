// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The pure-logic half of the RavenBSD resolver: the DNS wire codec, the multi-query
//! UDP/TCP engine, `getaddrinfo`/`getnameinfo`, the `inet_*` family and the
//! `/etc/resolv.conf`, `hosts`, `services`, `protocols` and `networks` parsers.
//!
//! Every external effect goes through the seam traits in [`traits`]: files through
//! [`raven_libc_nss::FileSource`], sockets through [`traits::DnsIo`], time through
//! [`traits::Clock`], random query IDs through [`traits::Entropy`], route probes through
//! [`traits::Probe`] and interface names through [`traits::IfNames`]. There is no global
//! resolver state, no cache and no clock-derived query ID.
//!
//! The crate is `no_std` + `alloc`, class P (`#![forbid(unsafe_code)]`).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)] // tests use fixed vectors and fail by panicking

extern crate alloc;

pub mod compat;
pub mod conf;
pub mod consts;
pub mod dbfiles;
pub mod engine;
pub mod errors;
pub mod gai;
pub mod hosts;
pub mod inet;
pub mod legacy;
pub mod linereader;
pub mod literal;
pub mod lookup;
pub mod msg;
pub mod name;
pub mod nameinfo;
pub mod netdb;
pub mod ns;
pub mod serv;
pub mod traits;
pub mod types;

pub use compat::{
    ResError, ResState, h_errno_from_rcode, herror, hstrerror, res_init, res_query,
    res_querydomain, res_search, res_send, res_state,
};
pub use conf::{ConfExt, ResolvConf, parse_options_ext, parse_resolv_conf};
pub use consts::*;
pub use dbfiles::{
    NetFile, Netent, ProtoFile, ServFile, endnetent, endprotoent, endservent, getnetent,
    getprotoent, getservent, parse_net_line, parse_serv_line, setnetent, setprotoent, setservent,
};
pub use engine::msend;
pub use errors::Eai;
pub use gai::{gai_strerror, getaddrinfo};
pub use hosts::{
    HostLine, HostsIter, endhostent, gethostent, parse_hosts, parse_hosts_line, sethostent,
};
pub use inet::{
    IN6ADDR_ANY, IN6ADDR_LOOPBACK, htonl, htons, inet_addr, inet_aton, inet_lnaof, inet_makeaddr,
    inet_netof, inet_network, inet_ntoa, inet_ntop, inet_pton, ntohl, ntohs,
};
pub use legacy::{
    HostEnt, PackedHostent, gethostbyaddr, gethostbyaddr_r, gethostbyname, gethostbyname_r,
    gethostbyname2, gethostbyname2_r, pack_hostent,
};
pub use linereader::LineReader;
pub use literal::{Literal, lookup_ipliteral};
pub use lookup::{Address, HostOrder, NameResult, OrderEntry, RFC6724_POLICY, lookup_name};
pub use msg::{dns_parse, dns_parse_offset, pack_edns0, question_matches, res_mkquery};
pub use name::{dn_comp, dn_expand, dn_skipname, is_valid_hostname};
pub use nameinfo::getnameinfo;
pub use netdb::{
    BUILTIN_PROTOCOLS, PackedProtoent, PackedServent, Proto, Protoent, Servent, getprotobyname,
    getprotobynumber, getservbyname, getservbyport, pack_protoent, pack_servent, parse_protocols,
};
pub use ns::{
    NS_FLAGDATA, NsMsg, NsRr, ns_get16, ns_get32, ns_initparse, ns_name_uncompress, ns_parserr,
    ns_put16, ns_put32, ns_skiprr,
};
pub use serv::{Service, lookup_serv};
pub use traits::{Clock, DnsIo, Entropy, Env, IfNames, Probe};
pub use types::{
    AddrInfoRec, Family, Hints, Interest, InterestKind, IpAddr, Recv, SockAddr, TcpSock, UdpSock,
};

/// The C `errno` type used by the file and socket seams (a positive Linux number).
pub type Errno = i32;

/// Read one byte, using `0` for a short buffer after the caller checked the bound.
#[inline]
pub(crate) fn u8_at(buf: &[u8], index: usize) -> u8 {
    buf.get(index).copied().unwrap_or(0)
}

/// Big-endian 16-bit read, or `None` when short.
#[inline]
pub(crate) fn be16_at(buf: &[u8], index: usize) -> Option<u16> {
    let hi = u16::from(buf.get(index).copied()?);
    let lo = u16::from(buf.get(index.checked_add(1)?).copied()?);
    Some((hi << 8) | lo)
}

/// Big-endian 32-bit read, or `None` when short.
#[inline]
pub(crate) fn be32_at(buf: &[u8], index: usize) -> Option<u32> {
    let a = u32::from(buf.get(index).copied()?);
    let b = u32::from(buf.get(index.checked_add(1)?).copied()?);
    let c = u32::from(buf.get(index.checked_add(2)?).copied()?);
    let d = u32::from(buf.get(index.checked_add(3)?).copied()?);
    Some((a << 24) | (b << 16) | (c << 8) | d)
}
