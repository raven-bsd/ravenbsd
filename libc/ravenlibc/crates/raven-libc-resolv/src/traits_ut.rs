// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the seam traits and the environment bundle.

use super::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use raven_libc_abi::errno::{EAGAIN, ENOENT, ENOTSUP};

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
    fn read_file(&self, path: &str, max: usize) -> Result<Vec<u8>, crate::Errno> {
        match self.data.get(path) {
            Some(bytes) => Ok(bytes.get(..bytes.len().min(max)).unwrap_or(&[]).to_vec()),
            None => Err(ENOENT),
        }
    }
}

struct Io;

impl DnsIo for Io {
    fn open_udp(&self, _family: Family) -> Result<UdpSock, crate::Errno> {
        Ok(UdpSock(1))
    }
    fn send_to(&self, _s: UdpSock, buf: &[u8], _to: &SockAddr) -> Result<usize, crate::Errno> {
        Ok(buf.len())
    }
    fn recv_from(&self, _s: UdpSock, _buf: &mut [u8]) -> Result<Recv, crate::Errno> {
        Err(EAGAIN)
    }
    fn open_tcp(&self, _to: &SockAddr) -> Result<TcpSock, crate::Errno> {
        Ok(TcpSock(2))
    }
    fn tcp_send(&self, _s: TcpSock, buf: &[u8]) -> Result<usize, crate::Errno> {
        Ok(buf.len())
    }
    fn tcp_recv(&self, _s: TcpSock, _buf: &mut [u8]) -> Result<usize, crate::Errno> {
        Ok(0)
    }
    fn wait(&self, _interests: &mut [Interest], _timeout_ms: u64) -> Result<(), crate::Errno> {
        Err(ENOTSUP)
    }
    fn close_udp(&self, _s: UdpSock) {}
    fn close_tcp(&self, _s: TcpSock) {}
}

struct FixedClock(u64);

impl Clock for FixedClock {
    fn monotonic_ms(&self) -> u64 {
        self.0
    }
}

struct CounterEntropy;

impl Entropy for CounterEntropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        for (index, slot) in buf.iter_mut().enumerate() {
            *slot = u8::try_from(index & 0xff).unwrap_or(0);
        }
        true
    }
}

struct NoProbe;

impl Probe for NoProbe {
    fn udp_route(&self, _dst: &SockAddr) -> Option<IpAddr> {
        None
    }
    fn family_usable(&self, family: Family) -> bool {
        family == Family::V4
    }
}

struct Names;

impl IfNames for Names {
    fn name_to_index(&self, name: &str) -> Option<u32> {
        if name == "lo0" { Some(1) } else { None }
    }
    fn index_to_name(&self, idx: u32) -> Option<String> {
        if idx == 1 {
            Some(String::from("lo0"))
        } else {
            None
        }
    }
}

fn env_with<'a>(files: &'a Files, io: &'a Io, clock: &'a FixedClock) -> Env<'a> {
    Env {
        files,
        io,
        clock,
        entropy: &CounterEntropy,
        probe: &NoProbe,
        ifnames: &Names,
    }
}

#[test]
fn env_reads_through_the_file_seam() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"127.0.0.1 localhost\n");
    let io = Io;
    let clock = FixedClock(0);
    let env = env_with(&files, &io, &clock);
    assert_eq!(
        env.read("/etc/hosts").expect("file"),
        b"127.0.0.1 localhost\n"
    );
}

#[test]
fn env_reports_a_missing_file() {
    let files = Files::new();
    let io = Io;
    let clock = FixedClock(0);
    let env = env_with(&files, &io, &clock);
    assert_eq!(env.read("/etc/none").unwrap_err(), ENOENT);
}

#[test]
fn env_random_reaches_the_entropy_seam() {
    let files = Files::new();
    let io = Io;
    let clock = FixedClock(0);
    let env = env_with(&files, &io, &clock);
    let mut bytes = [0u8; 4];
    assert!(env.random(&mut bytes));
    assert_eq!(bytes, [0, 1, 2, 3]);
}

#[test]
fn the_clock_seam_returns_its_value() {
    let clock = FixedClock(1234);
    assert_eq!(clock.monotonic_ms(), 1234);
}

#[test]
fn the_probe_can_say_no() {
    let probe = NoProbe;
    assert!(probe.family_usable(Family::V4));
    assert!(!probe.family_usable(Family::V6));
    assert_eq!(
        probe.udp_route(&SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53)),
        None
    );
}

#[test]
fn the_ifnames_seam_maps_names_and_indices() {
    let names = Names;
    assert_eq!(names.name_to_index("lo0"), Some(1));
    assert_eq!(names.name_to_index("eth0"), None);
    assert_eq!(names.index_to_name(1).as_deref(), Some("lo0"));
    assert_eq!(names.index_to_name(2), None);
}

#[test]
fn the_io_seam_is_usable_through_a_trait_object() {
    let io = Io;
    let dynamic: &dyn DnsIo = &io;
    let socket = dynamic.open_udp(Family::V4).expect("socket");
    assert_eq!(
        dynamic.send_to(socket, b"abc", &SockAddr::new(IpAddr::V4([1, 2, 3, 4]), 53)),
        Ok(3)
    );
    dynamic.close_udp(socket);
}

#[test]
fn the_file_seam_is_usable_through_a_trait_object() {
    let mut files = Files::new();
    files.put("/etc/services", b"http 80/tcp\n");
    let dynamic: &dyn FileSource = &files;
    assert_eq!(
        dynamic.read_file("/etc/services", 1024).expect("read"),
        b"http 80/tcp\n"
    );
}

#[test]
fn the_entropy_seam_can_fill_an_empty_buffer() {
    let mut empty: [u8; 0] = [];
    assert!(CounterEntropy.fill(&mut empty));
}

#[test]
fn env_carries_all_six_seams() {
    let files = Files::new();
    let io = Io;
    let clock = FixedClock(9);
    let env = env_with(&files, &io, &clock);
    assert_eq!(env.clock.monotonic_ms(), 9);
    assert!(env.probe.family_usable(Family::V4));
    assert_eq!(env.ifnames.name_to_index("lo0"), Some(1));
}
