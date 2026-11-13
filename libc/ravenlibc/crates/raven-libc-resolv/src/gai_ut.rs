// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `getaddrinfo` and the `gai_strerror` table.

use super::*;
use crate::consts::*;
use crate::types::{AddrInfoRec, IpAddr};
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
    opened: Cell<usize>,
}

impl Io {
    fn new() -> Self {
        Self {
            opened: Cell::new(0),
        }
    }
}

impl crate::traits::DnsIo for Io {
    fn open_udp(&self, _family: Family) -> Result<crate::types::UdpSock, crate::Errno> {
        self.opened.set(self.opened.get() + 1);
        Ok(crate::types::UdpSock(1))
    }
    fn send_to(
        &self,
        _s: crate::types::UdpSock,
        _buf: &[u8],
        _to: &SockAddr,
    ) -> Result<usize, crate::Errno> {
        Ok(0)
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
        _timeout_ms: u64,
    ) -> Result<(), crate::Errno> {
        Err(EAGAIN)
    }
    fn close_udp(&self, _s: crate::types::UdpSock) {}
    fn close_tcp(&self, _s: crate::types::TcpSock) {}
}

struct Clock;

impl crate::traits::Clock for Clock {
    fn monotonic_ms(&self) -> u64 {
        0
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

struct Probe {
    v6: bool,
}

impl crate::traits::Probe for Probe {
    fn udp_route(&self, _dst: &SockAddr) -> Option<IpAddr> {
        Some(IpAddr::V4([127, 0, 0, 1]))
    }
    fn family_usable(&self, family: Family) -> bool {
        match family {
            Family::V4 => true,
            Family::V6 => self.v6,
        }
    }
}

struct Names;

impl crate::traits::IfNames for Names {
    fn name_to_index(&self, _name: &str) -> Option<u32> {
        None
    }
    fn index_to_name(&self, _idx: u32) -> Option<String> {
        None
    }
}

fn files() -> Files {
    let mut files = Files::new();
    files.put(
        "/etc/hosts",
        b"192.0.2.1 server server.example\n::1 server\n127.0.0.1 localhost\n",
    );
    files.put("/etc/services", b"http 80/tcp\nhttp 80/udp\n");
    files
}

fn hints(flags: i32, family: i32, socktype: i32, protocol: i32) -> Hints {
    Hints {
        flags,
        family,
        socktype,
        protocol,
    }
}

fn order() -> HostOrder {
    HostOrder::defaults()
}

fn with_env<T>(files: &Files, io: &Io, probe: &Probe, f: impl FnOnce(&Env) -> T) -> T {
    let clock = Clock;
    let entropy = Entropy;
    let names = Names;
    let env = Env {
        files,
        io,
        clock: &clock,
        entropy: &entropy,
        probe,
        ifnames: &names,
    };
    f(&env)
}

fn lookup(
    files: &Files,
    host: Option<&str>,
    serv: Option<&str>,
    hints: &Hints,
    v6: bool,
) -> Result<Vec<AddrInfoRec>, Eai> {
    let io = Io::new();
    let probe = Probe { v6 };
    with_env(files, &io, &probe, |env| {
        getaddrinfo(host, serv, hints, env, &order())
    })
}

#[test]
fn gai_strerror_covers_the_musl_table() {
    let expected = [
        (-1, "Invalid flags"),
        (-2, "Name does not resolve"),
        (-3, "Try again"),
        (-4, "Non-recoverable error"),
        (-5, "Name has no usable address"),
        (-6, "Unrecognized address family or invalid length"),
        (-7, "Unrecognized socket type"),
        (-8, "Unrecognized service"),
        (-9, "Unknown error"),
        (-10, "Out of memory"),
        (-11, "System error"),
        (-12, "Overflow"),
    ];
    for (code, text) in expected {
        assert_eq!(gai_strerror(code), text);
    }
}

#[test]
fn gai_strerror_unknown_is_unknown() {
    assert_eq!(gai_strerror(0), "Unknown error");
    assert_eq!(gai_strerror(1), "Unknown error");
    assert_eq!(gai_strerror(-13), "Unknown error");
    assert_eq!(gai_strerror(i32::MIN), "Unknown error");
}

#[test]
fn no_host_and_no_serv_is_noname() {
    assert_eq!(
        lookup(&files(), None, None, &Hints::default(), true),
        Err(Eai::NoName)
    );
}

#[test]
fn an_unknown_flag_is_badflags() {
    let hints = hints(0x8000, 0, 0, 0);
    assert_eq!(
        lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true),
        Err(Eai::BadFlags)
    );
}

#[test]
fn an_unknown_family_is_family() {
    let hints = hints(0, 99, 0, 0);
    assert_eq!(
        lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true),
        Err(Eai::Family)
    );
}

#[test]
fn a_numeric_host_and_service_yield_two_records() {
    let records = lookup(
        &files(),
        Some("127.0.0.1"),
        Some("80"),
        &Hints::default(),
        true,
    )
    .expect("gai");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].addr.port, 80);
    assert_eq!(records[0].socktype, SOCK_STREAM);
    assert_eq!(records[0].protocol, IPPROTO_TCP);
    assert_eq!(records[1].socktype, SOCK_DGRAM);
    assert_eq!(records[1].protocol, IPPROTO_UDP);
    assert_eq!(records[0].family, AF_INET);
}

#[test]
fn the_hints_flags_are_copied_to_every_record() {
    let hints = hints(AI_CANONNAME, 0, 0, 0);
    let records = lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true).expect("gai");
    assert!(records.iter().all(|record| record.flags == AI_CANONNAME));
}

#[test]
fn a_stream_hint_yields_one_record() {
    let hints = hints(0, 0, SOCK_STREAM, 0);
    let records = lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true).expect("gai");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].protocol, IPPROTO_TCP);
}

#[test]
fn a_dgram_hint_yields_one_record() {
    let hints = hints(0, 0, SOCK_DGRAM, 0);
    let records = lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true).expect("gai");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].protocol, IPPROTO_UDP);
}

#[test]
fn a_tcp_protocol_hint_yields_one_record() {
    let hints = hints(0, 0, 0, IPPROTO_TCP);
    let records = lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true).expect("gai");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].protocol, IPPROTO_TCP);
}

#[test]
fn a_bad_socktype_with_a_name_is_service() {
    let hints = hints(0, 0, 99, 0);
    assert_eq!(
        lookup(&files(), Some("127.0.0.1"), Some("http"), &hints, true),
        Err(Eai::Service)
    );
}

#[test]
fn a_service_name_is_resolved() {
    let records = lookup(
        &files(),
        Some("127.0.0.1"),
        Some("http"),
        &Hints::default(),
        true,
    )
    .expect("gai");
    assert_eq!(records[0].addr.port, 80);
}

#[test]
fn an_unknown_service_is_service() {
    assert_eq!(
        lookup(
            &files(),
            Some("127.0.0.1"),
            Some("nope"),
            &Hints::default(),
            true
        ),
        Err(Eai::Service)
    );
}

#[test]
fn numericserv_rejects_a_name() {
    let hints = hints(AI_NUMERICSERV, 0, 0, 0);
    assert_eq!(
        lookup(&files(), Some("127.0.0.1"), Some("http"), &hints, true),
        Err(Eai::NoName)
    );
}

#[test]
fn a_hosts_hit_sets_the_canonical_name() {
    let hints = hints(AI_CANONNAME, 0, 0, 0);
    let records = lookup(&files(), Some("server.example"), Some("80"), &hints, true).expect("gai");
    assert_eq!(records[0].canonname.as_deref(), Some("server"));
}

#[test]
fn a_numeric_host_keeps_the_input_as_canonical_name() {
    let records = lookup(
        &files(),
        Some("192.0.2.9"),
        Some("80"),
        &Hints::default(),
        true,
    )
    .expect("gai");
    assert_eq!(records[0].canonname.as_deref(), Some("192.0.2.9"));
}

#[test]
fn numderichost_rejects_a_name() {
    let hints = hints(AI_NUMERICHOST, 0, 0, 0);
    assert_eq!(
        lookup(&files(), Some("server"), Some("80"), &hints, true),
        Err(Eai::NoName)
    );
}

#[test]
fn a_v4_literal_with_family_v6_is_nodata() {
    let hints = hints(0, AF_INET6, 0, 0);
    assert_eq!(
        lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true),
        Err(Eai::NoData)
    );
}

#[test]
fn passive_without_a_host_uses_unspecified_addresses() {
    let hints = hints(AI_PASSIVE, 0, 0, 0);
    let records = lookup(&files(), None, Some("80"), &hints, true).expect("gai");
    assert!(records.iter().all(|record| record.addr.ip.is_unspecified()));
}

#[test]
fn without_a_host_the_result_is_loopback() {
    let records = lookup(&files(), None, Some("80"), &Hints::default(), true).expect("gai");
    assert!(records.iter().any(|record| record.addr.ip.is_loopback()));
}

#[test]
fn a_host_without_a_service_uses_port_zero() {
    let records = lookup(&files(), Some("127.0.0.1"), None, &Hints::default(), true).expect("gai");
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| record.addr.port == 0));
}

#[test]
fn addrconfig_drops_an_unusable_v6_family() {
    let hints = hints(AI_ADDRCONFIG, 0, 0, 0);
    let records = lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, false).expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET));
}

#[test]
fn addrconfig_with_requested_v6_unusable_is_nodata() {
    let hints = hints(AI_ADDRCONFIG, AF_INET6, 0, 0);
    assert_eq!(
        lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, false),
        Err(Eai::NoData)
    );
}

#[test]
fn addrconfig_with_usable_families_keeps_v6() {
    let hints = hints(AI_ADDRCONFIG, AF_INET6, 0, 0);
    let records = lookup(&files(), Some("::1"), Some("80"), &hints, true).expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET6));
}

#[test]
fn v4mapped_turns_a_v4_literal_into_a_v6_record() {
    let hints = hints(AI_V4MAPPED, AF_INET6, 0, 0);
    let records = lookup(&files(), Some("127.0.0.1"), Some("80"), &hints, true).expect("gai");
    assert!(records.iter().all(|record| record.family == AF_INET6));
    assert!(
        records
            .iter()
            .all(|record| record.addr.ip.as_v4_mapped() == Some([127, 0, 0, 1]))
    );
}

#[test]
fn a_hosts_name_with_two_families_yields_both() {
    let records = lookup(
        &files(),
        Some("server"),
        Some("80"),
        &Hints::default(),
        true,
    )
    .expect("gai");
    assert!(records.iter().any(|record| record.family == AF_INET));
    assert!(records.iter().any(|record| record.family == AF_INET6));
}

#[test]
fn the_address_order_is_address_major() {
    let hints = hints(0, 0, SOCK_STREAM, 0);
    let records = lookup(&files(), Some("server"), None, &hints, true).expect("gai");
    assert!(records.len() >= 2);
    assert_eq!(records[0].socktype, SOCK_STREAM);
    assert_eq!(records[1].socktype, SOCK_STREAM);
    assert_ne!(records[0].family, records[1].family);
}
