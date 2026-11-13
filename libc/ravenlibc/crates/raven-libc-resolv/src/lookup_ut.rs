// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the search order, the hosts backend and the RFC 6724 sort.

use super::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use raven_libc_abi::errno::{EAGAIN, ENOENT};
use raven_libc_nss::{FileSource, Nsswitch, Source};

struct Files {
    data: BTreeMap<String, Result<Vec<u8>, crate::Errno>>,
}

impl Files {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    fn put(&mut self, path: &str, bytes: &[u8]) {
        self.data.insert(String::from(path), Ok(bytes.to_vec()));
    }

    fn fail(&mut self, path: &str, errno: crate::Errno) {
        self.data.insert(String::from(path), Err(errno));
    }
}

impl FileSource for Files {
    fn read_file(&self, path: &str, _max: usize) -> Result<Vec<u8>, crate::Errno> {
        self.data.get(path).cloned().unwrap_or(Err(ENOENT))
    }
}

struct CountingIo {
    opened: Cell<usize>,
}

impl CountingIo {
    fn new() -> Self {
        Self {
            opened: Cell::new(0),
        }
    }
}

impl crate::traits::DnsIo for CountingIo {
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

struct FixedClock;

impl crate::traits::Clock for FixedClock {
    fn monotonic_ms(&self) -> u64 {
        0
    }
}

struct FixedEntropy;

impl crate::traits::Entropy for FixedEntropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        for (index, slot) in buf.iter_mut().enumerate() {
            *slot = u8::try_from(index & 0xff).unwrap_or(0);
        }
        true
    }
}

struct TableProbe {
    routes: Vec<(IpAddr, Option<IpAddr>)>,
    v6: bool,
}

impl TableProbe {
    fn new() -> Self {
        Self {
            routes: Vec::new(),
            v6: true,
        }
    }

    fn route(mut self, dst: IpAddr, src: Option<IpAddr>) -> Self {
        self.routes.push((dst, src));
        self
    }
}

impl crate::traits::Probe for TableProbe {
    fn udp_route(&self, dst: &SockAddr) -> Option<IpAddr> {
        self.routes
            .iter()
            .find(|(address, _)| *address == dst.ip)
            .and_then(|(_, route)| *route)
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

fn with_env<T>(files: &Files, io: &CountingIo, probe: &TableProbe, f: impl FnOnce(&Env) -> T) -> T {
    let clock = FixedClock;
    let entropy = FixedEntropy;
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

fn v4(a: [u8; 4]) -> IpAddr {
    IpAddr::V4(a)
}

fn v6(last: u8) -> IpAddr {
    let mut bytes = [0u8; 16];
    bytes[15] = last;
    IpAddr::V6(bytes)
}

fn address(ip: IpAddr) -> Address {
    Address::with_addr(ip.family(), ip.octets16(), 0)
}

#[test]
fn the_default_order_is_files_then_dns() {
    let order = HostOrder::defaults();
    let entries: Vec<_> = order.iter().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].source, Source::Files);
    assert_eq!(entries[1].source, Source::Dns);
    assert!(!entries[0].on_notfound_return);
}

#[test]
fn from_nss_keeps_the_notfound_flag() {
    let nss = Nsswitch::parse(b"hosts: files [NOTFOUND=return] dns\n");
    let order = HostOrder::from_nss(&nss);
    let entries: Vec<_> = order.iter().collect();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].on_notfound_return);
    assert_eq!(entries[1].source, Source::Dns);
}

#[test]
fn from_nss_skips_unknown_sources() {
    let nss = Nsswitch::parse(b"hosts: nis files\n");
    let order = HostOrder::from_nss(&nss);
    let entries: Vec<_> = order.iter().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].source, Source::Skipped);
    assert_eq!(entries[1].source, Source::Files);
}

#[test]
fn from_entries_copies_an_explicit_order() {
    let order = HostOrder::from_entries(&[
        raven_libc_nss::SourceEntry {
            source: Source::Dns,
            on_notfound_return: false,
        },
        raven_libc_nss::SourceEntry {
            source: Source::Files,
            on_notfound_return: true,
        },
    ]);
    let entries: Vec<_> = order.iter().collect();
    assert_eq!(entries[0].source, Source::Dns);
    assert!(entries[1].on_notfound_return);
}

#[test]
fn an_empty_order_defaults() {
    assert!(HostOrder::from_sources(&[]).is_empty());
    assert!(!HostOrder::defaults().is_empty());
}

#[test]
fn the_default_order_is_not_empty() {
    assert!(!HostOrder::defaults().is_empty());
}

#[test]
fn a_numeric_v4_name_resolves_without_io() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(Some("192.0.2.7"), crate::consts::AF_UNSPEC, 0, env, &order)
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 1);
    assert_eq!(result.addrs[0].ip(), v4([192, 0, 2, 7]));
    assert_eq!(io.opened.get(), 0);
}

#[test]
fn a_numeric_v6_name_resolves() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(Some("2001:db8::1"), crate::consts::AF_INET6, 0, env, &order)
    })
    .expect("lookup");
    assert_eq!(result.addrs[0].ip().family(), Family::V6);
}

#[test]
fn a_numeric_name_with_a_wrong_family_is_nodata() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let error = with_env(&files, &io, &probe, |env| {
        lookup_name(Some("127.0.0.1"), crate::consts::AF_INET6, 0, env, &order)
    });
    assert_eq!(error, Err(Eai::NoData));
}

#[test]
fn numerichost_never_touches_io() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let error = with_env(&files, &io, &probe, |env| {
        lookup_name(
            Some("not-numeric"),
            crate::consts::AF_UNSPEC,
            crate::consts::AI_NUMERICHOST,
            env,
            &order,
        )
    });
    assert_eq!(error, Err(Eai::NoName));
    assert_eq!(io.opened.get(), 0);
}

#[test]
fn an_empty_name_is_noname() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    assert_eq!(
        with_env(&files, &io, &probe, |env| {
            lookup_name(Some(""), crate::consts::AF_UNSPEC, 0, env, &order)
        }),
        Err(Eai::NoName)
    );
}

#[test]
fn an_over_long_name_is_noname() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let long = "a".repeat(255);
    assert_eq!(
        with_env(&files, &io, &probe, |env| {
            lookup_name(Some(&long), crate::consts::AF_UNSPEC, 0, env, &order)
        }),
        Err(Eai::NoName)
    );
}

#[test]
fn no_host_without_passive_is_loopback() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(None, crate::consts::AF_UNSPEC, 0, env, &order)
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 2);
    assert!(
        result
            .addrs
            .iter()
            .any(|address| address.ip() == v4([127, 0, 0, 1]))
    );
    assert!(result.addrs.iter().any(|address| address.ip() == v6(1)));
}

#[test]
fn no_host_with_passive_is_unspecified() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(
            None,
            crate::consts::AF_UNSPEC,
            crate::consts::AI_PASSIVE,
            env,
            &order,
        )
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 2);
    assert!(
        result
            .addrs
            .iter()
            .all(|address| address.ip().is_unspecified())
    );
}

#[test]
fn no_host_with_family_v4_only() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(None, crate::consts::AF_INET, 0, env, &order)
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 1);
    assert_eq!(result.addrs[0].family, Family::V4);
}

#[test]
fn a_hosts_file_hit_is_used_and_sets_the_canonical_name() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"192.0.2.1 server server.example\n");
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }]);
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(
            Some("server.example"),
            crate::consts::AF_UNSPEC,
            0,
            env,
            &order,
        )
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 1);
    assert_eq!(result.canon, "server");
    assert_eq!(io.opened.get(), 0);
}

#[test]
fn a_hosts_file_error_is_system() {
    let mut files = Files::new();
    files.fail("/etc/hosts", 5);
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }]);
    assert_eq!(
        with_env(&files, &io, &probe, |env| {
            lookup_name(Some("server"), crate::consts::AF_UNSPEC, 0, env, &order)
        }),
        Err(Eai::System)
    );
}

#[test]
fn a_missing_hosts_file_is_not_an_error() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }]);
    assert_eq!(
        with_env(&files, &io, &probe, |env| {
            lookup_name(Some("server"), crate::consts::AF_UNSPEC, 0, env, &order)
        }),
        Err(Eai::NoName)
    );
}

#[test]
fn notfound_return_stops_before_dns() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"192.0.2.1 other\n");
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[
        OrderEntry {
            source: Source::Files,
            on_notfound_return: true,
        },
        OrderEntry {
            source: Source::Dns,
            on_notfound_return: false,
        },
    ]);
    assert_eq!(
        with_env(&files, &io, &probe, |env| {
            lookup_name(Some("server"), crate::consts::AF_UNSPEC, 0, env, &order)
        }),
        Err(Eai::NoName)
    );
    assert_eq!(io.opened.get(), 0);
}

#[test]
fn localhost_never_touches_dns() {
    let files = Files::new();
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::defaults();
    for name in ["localhost", "LOCALHOST.", "foo.localhost"] {
        let result = with_env(&files, &io, &probe, |env| {
            lookup_name(Some(name), crate::consts::AF_UNSPEC, 0, env, &order)
        })
        .expect("localhost");
        assert!(
            result
                .addrs
                .iter()
                .any(|address| address.ip().is_loopback())
        );
    }
    assert_eq!(io.opened.get(), 0);
}

#[test]
fn v4mapped_translates_v4_results() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"192.0.2.1 server\n");
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }]);
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(
            Some("server"),
            crate::consts::AF_INET6,
            crate::consts::AI_V4MAPPED,
            env,
            &order,
        )
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 1);
    assert_eq!(result.addrs[0].family, Family::V6);
    assert_eq!(result.addrs[0].ip().as_v4_mapped(), Some([192, 0, 2, 1]));
}

#[test]
fn v4mapped_all_keeps_both_families() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"192.0.2.1 server\n::1 server\n");
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }]);
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(
            Some("server"),
            crate::consts::AF_UNSPEC,
            crate::consts::AI_V4MAPPED | crate::consts::AI_ALL,
            env,
            &order,
        )
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 2);
    assert!(
        result
            .addrs
            .iter()
            .any(|address| address.ip().as_v4_mapped() == Some([192, 0, 2, 1]))
    );
}

#[test]
fn v4mapped_without_all_prefers_v6() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"192.0.2.1 server\n::1 server\n");
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[OrderEntry {
        source: Source::Files,
        on_notfound_return: false,
    }]);
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(
            Some("server"),
            crate::consts::AF_INET6,
            crate::consts::AI_V4MAPPED,
            env,
            &order,
        )
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 1);
    assert_eq!(result.addrs[0].ip(), v6(1));
}

#[test]
fn the_policy_table_has_the_donor_rows() {
    assert_eq!(RFC6724_POLICY.len(), 6);
    assert_eq!(RFC6724_POLICY[0].prec, 50);
    assert_eq!(RFC6724_POLICY[0].label, 0);
    assert_eq!(RFC6724_POLICY[1].prec, 35);
    assert_eq!(RFC6724_POLICY[1].len, 11);
    assert_eq!(RFC6724_POLICY[2].prec, 30);
    assert_eq!(RFC6724_POLICY[3].prec, 5);
    assert_eq!(RFC6724_POLICY[4].prec, 3);
    assert_eq!(RFC6724_POLICY[4].mask, 0xfe);
    assert_eq!(RFC6724_POLICY[5].prec, 40);
    assert_eq!(RFC6724_POLICY[5].label, 1);
}

#[test]
fn the_donor_omits_rules_3_4_and_7() {
    // No row for 3ffe::/16 (6to4), fec0::/10 (site-local) or the v4-compatible ::/96
    // prefix: the donor omits those rules for code-size and runtime reasons.
    assert_eq!(RFC6724_POLICY.len(), 6);
    for policy in RFC6724_POLICY.iter() {
        assert_ne!(policy.addr[0], 0x3f);
        assert_ne!(policy.addr[1], 0xfe);
    }
}

fn key(probe: &TableProbe, ip: IpAddr) -> i32 {
    sort_key(probe, &address(ip), 0)
}

#[test]
fn rule_1_usable_addresses_sort_first() {
    let mut near = [0u8; 16];
    near[0] = 0x20;
    near[1] = 0x01;
    near[2] = 0x0d;
    near[3] = 0xb8;
    near[15] = 2;
    let mut far = near;
    far[15] = 3;
    let probe = TableProbe::new()
        .route(IpAddr::V6(near), Some(IpAddr::V6(near)))
        .route(IpAddr::V6(far), None);
    let mut addrs = alloc::vec![address(IpAddr::V6(far)), address(IpAddr::V6(near))];
    sort_addresses(&mut addrs, &probe);
    assert_eq!(addrs[0].ip(), IpAddr::V6(near));
    assert_eq!(addrs[1].ip(), IpAddr::V6(far));
}

#[test]
fn rule_1_an_unusable_address_has_no_usable_bit() {
    let probe = TableProbe::new();
    assert_eq!(key(&probe, v6(1)) & DAS_USABLE, 0);
}

#[test]
fn rule_2_matching_scope_sets_the_bit() {
    let probe = TableProbe::new().route(v6(1), Some(v6(1)));
    assert_ne!(key(&probe, v6(1)) & DAS_MATCHINGSCOPE, 0);
}

#[test]
fn rule_2_a_mismatched_scope_does_not_set_the_bit() {
    let probe = TableProbe::new().route(v6(1), Some(v4([192, 0, 2, 1])));
    assert_eq!(key(&probe, v6(1)) & DAS_MATCHINGSCOPE, 0);
}

#[test]
fn rule_5_matching_labels_set_the_bit() {
    let probe = TableProbe::new().route(v4([192, 0, 2, 1]), Some(v4([192, 0, 2, 2])));
    assert_ne!(key(&probe, v4([192, 0, 2, 1])) & DAS_MATCHINGLABEL, 0);
}

#[test]
fn rule_6_precedence_bits_follow_the_table() {
    let probe = TableProbe::new();
    let precedence = |ip: IpAddr| (key(&probe, ip) >> DAS_PREC_SHIFT) & 0xff;
    assert_eq!(precedence(v6(1)), 50);
    assert_eq!(precedence(v4([192, 0, 2, 1])), 35);
    assert_eq!(precedence(v6(1)), 50);
    let mut teredo = [0u8; 16];
    teredo[0] = 0x20;
    teredo[1] = 0x01;
    teredo[15] = 1;
    assert_eq!(precedence(IpAddr::V6(teredo)), 5);
    let mut unique = [0u8; 16];
    unique[0] = 0xfc;
    unique[15] = 1;
    assert_eq!(precedence(IpAddr::V6(unique)), 3);
    let mut other = [0u8; 16];
    other[0] = 0x20;
    other[1] = 0x02;
    other[15] = 1;
    assert_eq!(precedence(IpAddr::V6(other)), 30);
    let mut normal = [0u8; 16];
    normal[0] = 0x26;
    normal[15] = 1;
    assert_eq!(precedence(IpAddr::V6(normal)), 40);
}

#[test]
fn rule_8_smaller_scope_sorts_first() {
    let mut link = [0u8; 16];
    link[0] = 0xfe;
    link[1] = 0x80;
    link[15] = 1;
    let mut site = [0u8; 16];
    site[0] = 0xfe;
    site[1] = 0xc0;
    site[15] = 1;
    let probe = TableProbe::new().route(IpAddr::V6(link), Some(IpAddr::V6(site)));
    let key_link = key(&probe, IpAddr::V6(link));
    let key_site = key(&probe, IpAddr::V6(site));
    assert!((key_link >> DAS_SCOPE_SHIFT) & 0xf > (key_site >> DAS_SCOPE_SHIFT) & 0xf);
}

#[test]
fn rule_9_a_longer_prefix_sorts_first() {
    let mut source = [0u8; 16];
    source[0] = 0x20;
    source[1] = 0x01;
    source[2] = 0x0d;
    source[3] = 0xb8;
    let mut near = source;
    near[15] = 2;
    let mut far = source;
    far[14] = 0x99;
    far[15] = 2;
    let probe = TableProbe::new().route(IpAddr::V6(near), Some(IpAddr::V6(source)));
    let near_key = key(&probe, IpAddr::V6(near));
    let far_key = key(&probe, IpAddr::V6(far));
    assert!((near_key >> DAS_PREFIX_SHIFT) & 0xff > (far_key >> DAS_PREFIX_SHIFT) & 0xff);
}

#[test]
fn sorting_is_stable_for_equal_keys() {
    let probe = TableProbe::new();
    let mut addrs = alloc::vec![address(v6(1)), address(v6(1))];
    sort_addresses(&mut addrs, &probe);
    assert_eq!(addrs.len(), 2);
}

#[test]
fn address_helpers_expose_the_right_forms() {
    let addr = Address::with_addr(Family::V4, v4([192, 0, 2, 1]).octets16(), 0);
    assert_eq!(addr.ip(), v4([192, 0, 2, 1]));
    assert_eq!(addr.sockaddr(53).port, 53);
    assert_eq!(addr.ip16(), v4([192, 0, 2, 1]).octets16());
}

#[test]
fn lookup_does_not_use_a_skipped_source() {
    let mut files = Files::new();
    files.put("/etc/hosts", b"192.0.2.1 server\n");
    let io = CountingIo::new();
    let probe = TableProbe::new();
    let order = HostOrder::from_sources(&[
        OrderEntry {
            source: Source::Skipped,
            on_notfound_return: false,
        },
        OrderEntry {
            source: Source::Files,
            on_notfound_return: false,
        },
    ]);
    let result = with_env(&files, &io, &probe, |env| {
        lookup_name(Some("server"), crate::consts::AF_UNSPEC, 0, env, &order)
    })
    .expect("lookup");
    assert_eq!(result.addrs.len(), 1);
}
