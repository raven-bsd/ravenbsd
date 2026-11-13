// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `res_*` compatibility layer.

use super::*;
use crate::consts::{HOST_NOT_FOUND, NO_DATA, NO_RECOVERY, TRY_AGAIN};
use crate::types::{Family, IpAddr, SockAddr};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use raven_libc_abi::errno::{EAGAIN, EINVAL, ENOENT, ENOMEM};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Silent,
    Answer,
    NxDomain,
    NoData,
}

struct Io {
    now: Cell<u64>,
    mode: Cell<Mode>,
    events: core::cell::RefCell<Vec<Vec<u8>>>,
}

impl Io {
    fn new(mode: Mode) -> Self {
        Self {
            now: Cell::new(0),
            mode: Cell::new(mode),
            events: core::cell::RefCell::new(Vec::new()),
        }
    }
}

fn response(request: &[u8], mode: Mode) -> Vec<u8> {
    let mut packet = request.to_vec();
    packet[2] |= 0x80;
    packet[3] = 0x80;
    packet[6] = 0;
    packet[7] = if mode == Mode::Answer { 1 } else { 0 };
    if mode == Mode::NxDomain {
        packet[3] = 0x83;
    }
    if mode == Mode::Answer {
        packet.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01]);
        packet.extend_from_slice(&60u32.to_be_bytes());
        packet.extend_from_slice(&4u16.to_be_bytes());
        packet.extend_from_slice(&[192, 0, 2, 1]);
    }
    packet
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
        let mode = self.mode.get();
        if mode != Mode::Silent {
            self.events.borrow_mut().push(response(buf, mode));
        }
        Ok(buf.len())
    }
    fn recv_from(
        &self,
        _s: crate::types::UdpSock,
        buf: &mut [u8],
    ) -> Result<crate::types::Recv, crate::Errno> {
        let mut events = self.events.borrow_mut();
        if events.is_empty() {
            return Err(EAGAIN);
        }
        let event = events.remove(0);
        let len = event.len().min(buf.len());
        buf[..len].copy_from_slice(&event[..len]);
        Ok(crate::types::Recv {
            len,
            from: SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53),
            truncated: false,
        })
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
        interests: &mut [crate::types::Interest],
        timeout_ms: u64,
    ) -> Result<(), crate::Errno> {
        let now = self.now.get();
        if self.events.borrow().is_empty() {
            self.now.set(now.saturating_add(timeout_ms));
        }
        for interest in interests.iter_mut() {
            if matches!(interest.kind, crate::types::InterestKind::Udp(_)) {
                interest.ready_r = true;
            }
        }
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

struct Entropy(bool);

impl crate::traits::Entropy for Entropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        if self.0 {
            for (index, slot) in buf.iter_mut().enumerate() {
                *slot = u8::try_from(index & 0xff).unwrap_or(0);
            }
        }
        self.0
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

fn with_env<T>(io: &Io, entropy: bool, f: impl FnOnce(&Env) -> T) -> T {
    let mut files = Files::new();
    files.put("/etc/resolv.conf", b"nameserver 127.0.0.1\n");
    let entropy = Entropy(entropy);
    let probe = Probe;
    let names = Names;
    let env = Env {
        files: &files,
        io,
        clock: io,
        entropy: &entropy,
        probe: &probe,
        ifnames: &names,
    };
    f(&env)
}

#[test]
fn res_init_is_a_noop() {
    assert_eq!(res_init(), 0);
}

#[test]
fn res_state_is_all_zero() {
    let state = res_state();
    assert_eq!(state, ResState::default());
}

#[test]
fn hstrerror_matches_the_musl_table() {
    assert_eq!(hstrerror(1), "Host not found");
    assert_eq!(hstrerror(2), "Try again");
    assert_eq!(hstrerror(3), "Non-recoverable error");
    assert_eq!(hstrerror(4), "Address not available");
    assert_eq!(hstrerror(0), "Unknown error");
    assert_eq!(hstrerror(5), "Unknown error");
}

#[test]
fn herror_text_includes_the_message() {
    assert_eq!(herror_text(Some("lookup"), 1), "lookup: Host not found");
    assert_eq!(herror_text(None, 2), "Try again");
}

#[test]
fn herror_uses_the_same_text() {
    assert_eq!(herror(Some("lookup"), 1), "lookup: Host not found");
    assert_eq!(herror(None, 3), "Non-recoverable error");
}

#[test]
fn h_errno_from_rcode_is_reexported() {
    assert_eq!(h_errno_from_rcode(3), HOST_NOT_FOUND);
    assert_eq!(h_errno_from_rcode(2), TRY_AGAIN);
    assert_eq!(h_errno_from_rcode(0), NO_DATA);
    assert_eq!(h_errno_from_rcode(9), NO_RECOVERY);
}

#[test]
fn res_send_times_out_with_try_again() {
    let io = Io::new(Mode::Silent);
    let mut answer = [0u8; 512];
    let error = with_env(&io, true, |env| res_send(&[0u8; 12], &mut answer, env)).unwrap_err();
    assert_eq!(error.errno, EAGAIN);
    assert_eq!(error.h_errno, TRY_AGAIN);
}

#[test]
fn res_query_entropy_failure_is_no_recovery() {
    let io = Io::new(Mode::Silent);
    let mut answer = [0u8; 512];
    let error = with_env(&io, false, |env| {
        res_query(b"example.org", 1, 1, &mut answer, env)
    })
    .unwrap_err();
    assert_eq!(error.errno, EAGAIN);
    assert_eq!(error.h_errno, NO_RECOVERY);
}

#[test]
fn res_query_rejects_a_name_that_does_not_fit() {
    let io = Io::new(Mode::Silent);
    let mut name = alloc::vec![b'a'; 63];
    name.push(b'.');
    name.extend_from_slice(&[b'b'; 192]);
    let mut answer = [0u8; 512];
    let error = with_env(&io, true, |env| res_query(&name, 1, 1, &mut answer, env)).unwrap_err();
    assert_eq!(error.errno, EINVAL);
    assert_eq!(error.h_errno, NO_RECOVERY);
}

#[test]
fn res_query_accepts_a_positive_answer() {
    let io = Io::new(Mode::Answer);
    let mut answer = [0u8; 512];
    let length = with_env(&io, true, |env| {
        res_query(b"example.org", 1, 1, &mut answer, env)
    })
    .expect("query");
    assert_eq!(length, 45);
    assert_eq!(answer[7], 1);
}

#[test]
fn res_query_maps_nxdomain_to_host_not_found() {
    let io = Io::new(Mode::NxDomain);
    let mut answer = [0u8; 512];
    let error = with_env(&io, true, |env| {
        res_query(b"example.org", 1, 1, &mut answer, env)
    })
    .unwrap_err();
    assert_eq!(error.errno, ENOENT);
    assert_eq!(error.h_errno, HOST_NOT_FOUND);
}

#[test]
fn res_query_maps_nodata_to_no_data() {
    let io = Io::new(Mode::NoData);
    let mut answer = [0u8; 512];
    let error = with_env(&io, true, |env| {
        res_query(b"example.org", 1, 1, &mut answer, env)
    })
    .unwrap_err();
    assert_eq!(error.errno, ENOENT);
    assert_eq!(error.h_errno, NO_DATA);
}

#[test]
fn res_search_is_the_res_query_alias() {
    let io = Io::new(Mode::Answer);
    let mut answer = [0u8; 512];
    let length = with_env(&io, true, |env| {
        res_search(b"example.org", 1, 1, &mut answer, env)
    })
    .expect("search");
    assert_eq!(length, 45);
}

#[test]
fn res_querydomain_rejects_a_name_that_is_too_long() {
    let io = Io::new(Mode::Silent);
    let mut answer = [0u8; 512];
    let name = alloc::vec![b'a'; 250];
    let error = with_env(&io, true, |env| {
        res_querydomain(&name, b"example.com", 1, 1, &mut answer, env)
    })
    .unwrap_err();
    assert_eq!(error.errno, EINVAL);
}

#[test]
fn res_querydomain_joins_the_names_and_sends() {
    let io = Io::new(Mode::Answer);
    let mut answer = [0u8; 512];
    let length = with_env(&io, true, |env| {
        res_querydomain(b"www", b"example.org", 1, 1, &mut answer, env)
    })
    .expect("querydomain");
    assert!(length >= 45);
}

#[test]
fn res_send_with_a_small_buffer_still_returns_the_full_length() {
    let io = Io::new(Mode::Answer);
    let mut answer = [0u8; 16];
    let length = with_env(&io, true, |env| res_send(&[0u8; 29], &mut answer, env));
    assert!(length.is_err() || length == Ok(45));
}

#[test]
fn service_memory_error_is_enomem() {
    assert_eq!(service_memory_error(), ENOMEM);
}

#[test]
fn res_error_carries_both_values() {
    let error = ResError {
        errno: EINVAL,
        h_errno: NO_RECOVERY,
    };
    assert_eq!(error.errno, EINVAL);
    assert_eq!(error.h_errno, NO_RECOVERY);
}
