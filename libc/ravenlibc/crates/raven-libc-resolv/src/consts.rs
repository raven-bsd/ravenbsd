// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Every limit and every protocol constant of the resolver.
//!
//! The values are the ticket's limits table (section 4.1) plus the Linux `AF_*` numbers and
//! the musl `netdb.h` flag values.

/// `AF_UNSPEC`.
pub const AF_UNSPEC: i32 = 0;
/// `AF_INET` (Linux).
pub const AF_INET: i32 = 2;
/// `AF_INET6` (Linux).
pub const AF_INET6: i32 = 10;

/// Maximum configured nameservers.
pub const MAXNS: usize = 3;
/// Maximum addresses returned by one `getaddrinfo`.
pub const MAXADDRS: usize = 48;
/// Maximum services returned by one `getaddrinfo`.
pub const MAXSERVS: usize = 2;

/// Longest DNS name in text form, without the trailing dot.
pub const DNS_NAME_MAX: usize = 253;
/// Longest DNS name in text form, with the trailing dot.
pub const DNS_NAME_TEXT_MAX: usize = 254;
/// Longest DNS label.
pub const DNS_LABEL_MAX: usize = 63;
/// A label length byte above this value is a compression pointer.
pub const DNS_LABEL_PTR: u8 = 0xc0;

/// The standard answer buffer size (`ABUF_SIZE`).
pub const ABUF_SIZE: usize = 4800;
/// Largest UDP query the engine builds.
pub const UDP_QUERY_MAX: usize = 512;
/// Largest TCP DNS message.
pub const TCP_MSG_MAX: usize = 65535;

/// Longest accepted `/etc/resolv.conf` line.
pub const RESOLV_CONF_LINE_MAX: usize = 256;
/// Longest accepted `/etc/hosts` line.
pub const HOSTS_LINE_MAX: usize = 512;
/// Longest accepted services/protocols/networks line (the donor `fgets` size).
pub const DB_LINE_MAX: usize = 128;
/// Longest accepted passwd/group/master.passwd line (shared with `raven-libc-pwd`).
pub const PASSWD_LINE_MAX: usize = 4096;
/// File read cap passed to `FileSource::read_file`.
pub const FILE_READ_MAX: usize = 1024 * 1024;

/// `NI_MAXHOST`.
pub const NI_MAXHOST: usize = 255;
/// `NI_MAXSERV`.
pub const NI_MAXSERV: usize = 32;
/// `IF_NAMESIZE`.
pub const IF_NAMESIZE: usize = 16;

/// `AI_PASSIVE`.
pub const AI_PASSIVE: i32 = 1;
/// `AI_CANONNAME`.
pub const AI_CANONNAME: i32 = 2;
/// `AI_NUMERICHOST`.
pub const AI_NUMERICHOST: i32 = 4;
/// `AI_V4MAPPED`.
pub const AI_V4MAPPED: i32 = 8;
/// `AI_ALL`.
pub const AI_ALL: i32 = 0x10;
/// `AI_ADDRCONFIG`.
pub const AI_ADDRCONFIG: i32 = 0x20;
/// `AI_NUMERICSERV`.
pub const AI_NUMERICSERV: i32 = 0x400;

/// The flags `getaddrinfo` knows about.
pub const AI_MASK: i32 = AI_PASSIVE
    | AI_CANONNAME
    | AI_NUMERICHOST
    | AI_V4MAPPED
    | AI_ALL
    | AI_ADDRCONFIG
    | AI_NUMERICSERV;

/// `NI_NUMERICHOST`.
pub const NI_NUMERICHOST: i32 = 1;
/// `NI_NUMERICSERV`.
pub const NI_NUMERICSERV: i32 = 2;
/// `NI_NOFQDN`.
pub const NI_NOFQDN: i32 = 4;
/// `NI_NAMEREQD`.
pub const NI_NAMEREQD: i32 = 8;
/// `NI_DGRAM`.
pub const NI_DGRAM: i32 = 16;
/// `NI_NUMERICSCOPE`.
pub const NI_NUMERICSCOPE: i32 = 0x100;

/// The flags `getnameinfo` knows about.
pub const NI_MASK: i32 =
    NI_NUMERICHOST | NI_NUMERICSERV | NI_NOFQDN | NI_NAMEREQD | NI_DGRAM | NI_NUMERICSCOPE;

/// DNS record type A.
pub const RR_A: u8 = 1;
/// DNS record type CNAME.
pub const RR_CNAME: u8 = 5;
/// DNS record type PTR.
pub const RR_PTR: u8 = 12;
/// DNS record type AAAA.
pub const RR_AAAA: u8 = 28;
/// DNS pseudo-record type OPT.
pub const RR_OPT: u8 = 41;

/// DNS class IN.
pub const CLASS_IN: u16 = 1;

/// The DNS server port.
pub const DNS_PORT: u16 = 53;
/// EDNS0 UDP payload size used when `options edns0` is set.
pub const EDNS0_UDP_SIZE: u16 = 1232;
/// The byte length of one OPT pseudo-record.
pub const EDNS0_LEN: usize = 11;

/// `HOST_NOT_FOUND`.
pub const HOST_NOT_FOUND: i32 = 1;
/// `TRY_AGAIN`.
pub const TRY_AGAIN: i32 = 2;
/// `NO_RECOVERY`.
pub const NO_RECOVERY: i32 = 3;
/// `NO_DATA`.
pub const NO_DATA: i32 = 4;

/// `IPPROTO_TCP`.
pub const IPPROTO_TCP: i32 = 6;
/// `IPPROTO_UDP`.
pub const IPPROTO_UDP: i32 = 17;

/// `SOCK_STREAM`.
pub const SOCK_STREAM: i32 = 1;
/// `SOCK_DGRAM`.
pub const SOCK_DGRAM: i32 = 2;
/// `SOCK_RAW`.
pub const SOCK_RAW: i32 = 3;

/// RFC 1035 header size.
pub const DNS_HEADER_LEN: usize = 12;
/// The fixed part of a resource record after the name.
pub const DNS_RR_FIXED: usize = 10;
/// The fixed part of a question after the name.
pub const DNS_QUESTION_FIXED: usize = 4;

/// The maximum entries kept by `HostOrder`.
pub const MAX_ORDER: usize = 8;

/// The engine step bound that guarantees termination even with a broken `DnsIo`.
pub const ENGINE_MAX_STEPS: usize = 4096;

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
