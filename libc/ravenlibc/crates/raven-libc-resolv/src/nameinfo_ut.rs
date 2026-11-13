// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `getnameinfo`, the PTR name construction and the numeric fallbacks.

use super::*;
use crate::types::IpAddr;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use raven_libc_abi::errno::{EAGAIN, ENOENT};
use raven_libc_nss::FileSource;

struct Files {
    data: BTreeMap<String, Vec<u8>>,
}

impl Files {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) {
        self.data.insert(String::from(path), bytes.to_vec());
    }
}

impl FileSource for Files {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, crate::Errno> {
        self.data.get(path).cloned().ok_or(ENOENT)
    }
}

struct Io {
    now: Cell<u64>,
}

impl Io {
    fn new() -> Self {
        Self { now: Cell::new(0) }
    }
}

impl crate::traits::DnsIo for Io {
    fn open_udp(&self, _family: Family) -> Result<crate::types::UdpSock, crate::Errno> {
        Ok(crate::types::UdpSock(1))
    }
    fn send_to(
        &self,
        _s: crate::types::UdpSock,
        buf: &[u8],
        _to: &SockAddr,
    ) -> Result<usize, crate::Errno> {
        Ok(buf.len())
    }
    fn recv_from(
        &self,
        _s: crate::types::UdpSock,
        _buf: &mut [u8],
    ) -> Result<crate::types::Recv, crate::Errno> {
        Err(EAGAIN)
    }
    fn open_tcp(&self, _to: &SockAddr) -> Result<crate::types::TcpSock, crate::Errno> {
        Ok(crate::types::TcpSock(1))
    }
    fn tcp_send(&self, _s: crate::types::TcpSock, _buf: &[u8]) -> Result<usize, crate::Errno> {
        Ok(0)
    }
    fn tcp_recv(&self, _s: crate::types::TcpSock, _buf: &mut [u8]) -> Result<usize, crate::Errno> {
        Ok(0)
    }
    fn wait(
        &self,
        _interests: &mut [crate::types::Interest],
        timeout_ms: u64,
    ) -> Result<(), crate::Errno> {
        self.now.set(self.now.get().saturating_add(timeout_ms));
        Ok(())
    }
    fn close_udp(&self, _s: crate::types::UdpSock) {}
    fn close_tcp(&self, _s: crate::types::TcpSock) {}
}

impl crate::traits::Clock for Io {
    fn monotonic_ms(&self) -> u64 {
        self.now.get()
    }
}

struct Entropy;

impl crate::traits::Entropy for Entropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        for (index, slot) in buf.iter_mut().enumerate() {
            *slot = u8::try_from(index & 0xff).unwrap_or(0);
        }
        true
    }
}

struct Probe;

impl crate::traits::Probe for Probe {
    fn udp_route(&self, _dst: &SockAddr) -> Option<IpAddr> {
        Some(IpAddr::V4([127, 0, 0, 1]))
    }
    fn family_usable(&self, family: Family) -> bool {
        family == Family::V4
    }
}

struct Names;

impl crate::traits::IfNames for Names {
    fn name_to_index(&self, _name: &str) -> Option<u32> {
        None
    }
    fn index_to_name(&self, idx: u32) -> Option<String> {
        if idx == 7 {
            Some(String::from("eth0"))
        } else {
            None
        }
    }
}

fn files() -> Files {
    let mut files = Files::new();
    files.put(
        "/etc/hosts",
        b"127.0.0.1 localhost\n192.0.2.1 fqdn.example.com alias\n",
    );
    files.put("/etc/services", b"http 80/tcp\nhttp 80/udp\n");
    files
}

fn call(
    files: &Files,
    addr: &SockAddr,
    hostlen: usize,
    servlen: usize,
    flags: i32,
) -> Result<(Option<String>, Option<String>), Eai> {
    let io = Io::new();
    let entropy = Entropy;
    let probe = Probe;
    let names = Names;
    let env = Env {
        files,
        io: &io,
        clock: &io,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    getnameinfo(
        addr,
        hostlen,
        servlen,
        flags,
        &env,
        &crate::lookup::HostOrder::defaults(),
    )
}

#[test]
fn the_ipv4_ptr_name() {
    assert_eq!(
        ptr_name(&SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0)),
        "1.2.0.192.in-addr.arpa"
    );
}

#[test]
fn the_ipv6_ptr_name_matches_rfc_3596() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0x43;
    bytes[1] = 0x21;
    bytes[5] = 0x01;
    bytes[7] = 0x02;
    bytes[9] = 0x03;
    bytes[11] = 0x04;
    bytes[12] = 0x05;
    bytes[13] = 0x67;
    bytes[14] = 0x89;
    bytes[15] = 0xab;
    assert_eq!(
        ptr_name(&SockAddr::new(IpAddr::V6(bytes), 0)),
        "b.a.9.8.7.6.5.0.4.0.0.0.3.0.0.0.2.0.0.0.1.0.0.0.0.0.0.0.1.2.3.4.ip6.arpa"
    );
}

#[test]
fn a_v4_mapped_address_uses_the_v4_ptr_name() {
    let mut bytes = [0u8; 16];
    bytes[10] = 0xff;
    bytes[11] = 0xff;
    bytes[12] = 192;
    bytes[13] = 0;
    bytes[14] = 2;
    bytes[15] = 1;
    assert_eq!(
        ptr_name(&SockAddr::new(IpAddr::V6(bytes), 0)),
        "1.2.0.192.in-addr.arpa"
    );
}

#[test]
fn a_hosts_entry_is_used_for_the_reverse_name() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 0),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("localhost"));
}

#[test]
fn a_hosts_alias_does_not_replace_the_canonical_name() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("fqdn.example.com"));
}

#[test]
fn a_missing_hosts_entry_falls_back_to_numeric() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"127.0.0.1 localhost\n");
    let (host, _) = call(
        &files,
        &SockAddr::new(IpAddr::V4([198, 51, 100, 1]), 0),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("198.51.100.1"));
}

#[test]
fn numerichost_skips_the_hosts_file() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        NI_NUMERICHOST,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("192.0.2.1"));
}

#[test]
fn namereqd_fails_without_a_name() {
    let files = Files::new();
    let error = call(
        &files,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        NI_NAMEREQD,
    );
    assert_eq!(error, Err(Eai::NoName));
}

#[test]
fn nofqdn_strips_the_domain() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 0),
        255,
        0,
        NI_NOFQDN,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("fqdn"));
}

#[test]
fn a_numeric_service_is_formatted() {
    let (_, service) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 80),
        0,
        32,
        NI_NUMERICSERV,
    )
    .expect("nameinfo");
    assert_eq!(service.as_deref(), Some("80"));
}

#[test]
fn a_named_service_is_reversed() {
    let (_, service) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 80),
        0,
        32,
        0,
    )
    .expect("nameinfo");
    assert_eq!(service.as_deref(), Some("http"));
}

#[test]
fn a_udp_service_uses_the_dgram_flag() {
    let mut files = Files::new();
    files.put("/etc/services", b"http 80/tcp\ndns 53/udp\n");
    let (_, service) = call(
        &files,
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 53),
        0,
        32,
        NI_DGRAM,
    )
    .expect("nameinfo");
    assert_eq!(service.as_deref(), Some("dns"));
}

#[test]
fn an_unknown_service_falls_back_to_numeric() {
    let (_, service) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([192, 0, 2, 1]), 9999),
        0,
        32,
        0,
    )
    .expect("nameinfo");
    assert_eq!(service.as_deref(), Some("9999"));
}

#[test]
fn a_zero_host_buffer_skips_the_host() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 0),
        0,
        0,
        0,
    )
    .expect("nameinfo");
    assert!(host.is_none());
}

#[test]
fn a_zero_service_buffer_skips_the_service() {
    let (_, service) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 80),
        255,
        0,
        0,
    )
    .expect("nameinfo");
    assert!(service.is_none());
}

#[test]
fn an_exact_host_buffer_fits() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 0),
        10,
        0,
        0,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("localhost"));
}

#[test]
fn a_one_short_host_buffer_overflows() {
    let error = call(
        &files(),
        &SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 0),
        9,
        0,
        0,
    );
    assert_eq!(error, Err(Eai::Overflow));
}

#[test]
fn a_one_short_service_buffer_overflows() {
    let error = call(
        &files(),
        &SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 80),
        0,
        2,
        NI_NUMERICSERV,
    );
    assert_eq!(error, Err(Eai::Overflow));
}

#[test]
fn a_link_local_scope_is_named() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xfe;
    bytes[1] = 0x80;
    bytes[15] = 1;
    let (host, _) = call(
        &files(),
        &SockAddr::with_scope(IpAddr::V6(bytes), 0, 7),
        255,
        0,
        NI_NUMERICHOST,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("fe80::1%eth0"));
}

#[test]
fn a_link_local_scope_is_numeric_with_the_flag() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xfe;
    bytes[1] = 0x80;
    bytes[15] = 1;
    let (host, _) = call(
        &files(),
        &SockAddr::with_scope(IpAddr::V6(bytes), 0, 7),
        255,
        0,
        NI_NUMERICHOST | NI_NUMERICSCOPE,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("fe80::1%7"));
}

#[test]
fn nofqdn_does_not_strip_a_numeric_address() {
    let (host, _) = call(
        &files(),
        &SockAddr::new(IpAddr::V4([198, 51, 100, 1]), 0),
        255,
        0,
        NI_NUMERICHOST | NI_NOFQDN,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("198.51.100.1"));
}

#[test]
fn a_global_scope_is_numeric() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0x20;
    bytes[1] = 0x01;
    bytes[15] = 1;
    let (host, _) = call(
        &files(),
        &SockAddr::with_scope(IpAddr::V6(bytes), 0, 7),
        255,
        0,
        NI_NUMERICHOST,
    )
    .expect("nameinfo");
    assert_eq!(host.as_deref(), Some("2001::1%7"));
}
