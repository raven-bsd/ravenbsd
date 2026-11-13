// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the legacy `gethostby*` wrappers and their packing.

use super::*;
use crate::consts::{AF_INET, AF_INET6, HOST_NOT_FOUND, NO_RECOVERY};
use crate::lookup::{HostOrder, OrderEntry};
use crate::types::Family;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use raven_libc_abi::errno::{EAGAIN, ENOENT, ERANGE};
use raven_libc_nss::{FileSource, Source};

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

struct Io;

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
        _timeout_ms: u64,
    ) -> Result<(), crate::Errno> {
        Ok(())
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

struct Probe;

impl crate::traits::Probe for Probe {
    fn udp_route(&self, _dst: &SockAddr) -> Option<IpAddr> {
        None
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
    fn index_to_name(&self, _idx: u32) -> Option<String> {
        None
    }
}

fn files() -> Files {
    let mut files = Files::new();
    files.put(
        "/etc/hosts",
        b"192.0.2.1 server.example server\n::1 server.example\n",
    );
    files
}

fn files_only() -> HostOrder {
    HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }])
}

fn with_env<T>(files: &Files, f: impl FnOnce(&Env) -> T) -> T {
    let io = Io;
    let clock = Clock;
    let entropy = Entropy;
    let probe = Probe;
    let names = Names;
    let env = Env {
        files,
        io: &io,
        clock: &clock,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    f(&env)
}

#[test]
fn a_dotted_quad_is_returned_as_a_literal() {
    let files = Files::new();
    let entry =
        with_env(&files, |env| gethostbyname("127.0.0.1", env, &files_only())).expect("host");
    assert_eq!(entry.name, "127.0.0.1");
    assert_eq!(entry.addr_type, AF_INET);
    assert_eq!(entry.addrs, alloc::vec![alloc::vec![127, 0, 0, 1]]);
}

#[test]
fn a_hosts_entry_sets_the_canonical_name_and_alias() {
    let files = files();
    let entry = with_env(&files, |env| {
        gethostbyname2("server", AF_INET, env, &files_only())
    })
    .expect("host");
    assert_eq!(entry.name, "server.example");
    assert_eq!(entry.aliases, alloc::vec![String::from("server")]);
    assert_eq!(entry.addrs, alloc::vec![alloc::vec![192, 0, 2, 1]]);
}

#[test]
fn an_ipv6_host_has_sixteen_byte_addresses() {
    let files = files();
    let entry = with_env(&files, |env| {
        gethostbyname2("server.example", AF_INET6, env, &files_only())
    })
    .expect("host");
    assert_eq!(entry.addr_type, AF_INET6);
    assert_eq!(entry.addrs[0].len(), 16);
}

#[test]
fn a_missing_name_is_host_not_found() {
    let files = files();
    let error = with_env(&files, |env| gethostbyname("nope", env, &files_only())).unwrap_err();
    assert_eq!(error, HOST_NOT_FOUND);
}

#[test]
fn gethostbyaddr_uses_the_hosts_reverse_name() {
    let files = files();
    let entry = with_env(&files, |env| {
        gethostbyaddr(&[192, 0, 2, 1], AF_INET, env, &files_only())
    })
    .expect("address");
    assert_eq!(entry.name, "server.example");
    assert_eq!(entry.addrs, alloc::vec![alloc::vec![192, 0, 2, 1]]);
}

#[test]
fn gethostbyaddr_rejects_a_wrong_length() {
    let files = Files::new();
    let error = with_env(&files, |env| {
        gethostbyaddr(&[127, 0, 0], AF_INET, env, &files_only())
    })
    .unwrap_err();
    assert_eq!(error, NO_RECOVERY);
}

#[test]
fn pack_hostent_writes_name_aliases_and_addresses() {
    let entry = HostEnt {
        name: String::from("server.example"),
        aliases: alloc::vec![String::from("server")],
        addr_type: AF_INET,
        addrs: alloc::vec![alloc::vec![192, 0, 2, 1]],
    };
    let mut buf = [0u8; 128];
    let packed = pack_hostent(&entry, &mut buf).expect("pack");
    assert_eq!(&buf[..15], b"server.example\0");
    assert_eq!(packed.name_len, 14);
    assert_eq!(packed.alias_count, 1);
    assert_eq!(packed.addr_count, 1);
    assert_eq!(packed.addr_len, 4);
    assert_eq!(&buf[15..22], b"server\0");
    assert_eq!(&buf[22..26], &[192, 0, 2, 1]);
    assert_eq!(
        &buf[packed.alias_table_off..packed.alias_table_off + 4],
        &15u32.to_le_bytes()
    );
    assert_eq!(
        &buf[packed.addr_table_off..packed.addr_table_off + 4],
        &22u32.to_le_bytes()
    );
}

#[test]
fn pack_hostent_returns_range_when_too_small() {
    let entry = HostEnt {
        name: String::from("server.example"),
        aliases: Vec::new(),
        addr_type: AF_INET,
        addrs: alloc::vec![alloc::vec![192, 0, 2, 1]],
    };
    assert_eq!(pack_hostent(&entry, &mut [0u8; 8]), Err(ERANGE));
}

#[test]
fn gethostbyname2_r_packs_the_result() {
    let files = files();
    let mut buf = [0u8; 128];
    let packed = with_env(&files, |env| {
        gethostbyname2_r("server.example", AF_INET, &mut buf, env, &files_only())
    })
    .expect("lookup")
    .expect("entry");
    assert_eq!(packed.addr_count, 1);
    assert_eq!(&buf[..14], b"server.example");
}

#[test]
fn gethostbyname2_r_returns_none_when_not_found() {
    let files = files();
    let mut buf = [0u8; 128];
    let result = with_env(&files, |env| {
        gethostbyname2_r("nope", AF_INET, &mut buf, env, &files_only())
    })
    .expect("lookup");
    assert!(result.is_none());
}

#[test]
fn gethostbyname2_r_returns_range_for_a_small_buffer() {
    let files = files();
    let mut buf = [0u8; 8];
    assert_eq!(
        with_env(&files, |env| {
            gethostbyname2_r("server.example", AF_INET, &mut buf, env, &files_only())
        }),
        Err(ERANGE)
    );
}

#[test]
fn gethostbyname_r_is_the_v4_form() {
    let files = files();
    let mut buf = [0u8; 128];
    let packed = with_env(&files, |env| {
        gethostbyname_r("server.example", &mut buf, env, &files_only())
    })
    .expect("lookup")
    .expect("entry");
    assert_eq!(packed.addr_len, 4);
}

#[test]
fn gethostbyaddr_r_packs_the_reverse_name() {
    let files = files();
    let mut buf = [0u8; 128];
    let packed = with_env(&files, |env| {
        gethostbyaddr_r(&[192, 0, 2, 1], AF_INET, &mut buf, env, &files_only())
    })
    .expect("lookup")
    .expect("entry");
    assert_eq!(&buf[..packed.name_len], b"server.example");
}

#[test]
fn gethostbyaddr_r_rejects_a_wrong_length() {
    let files = Files::new();
    let mut buf = [0u8; 16];
    assert_eq!(
        with_env(&files, |env| {
            gethostbyaddr_r(&[1, 2, 3], AF_INET, &mut buf, env, &files_only())
        }),
        Err(raven_libc_abi::errno::EINVAL)
    );
}

#[test]
fn herror_for_maps_the_eai_values() {
    assert_eq!(herror_for(Eai::NoName), HOST_NOT_FOUND);
    assert_eq!(herror_for(Eai::Again), TRY_AGAIN);
    assert_eq!(herror_for(Eai::Fail), NO_RECOVERY);
}
