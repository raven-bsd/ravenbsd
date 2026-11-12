// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One test per C24 accepted deviation, plus the measured defaults that the
//! oracle table compares. These are pins, not stack fixes.

use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetFrame};
use raven_tcpip_wire::ipv4::Ipv4Packet;
use raven_tcpip_wire::tcp::TcpPacket;

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];

fn stack(endpoint: Endpoint) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), 1, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    stack
}

#[test]
fn accepted_deviation_ecn_is_offered_by_default() {
    let (a, _b) = Link::new(LinkConfig::default(), 9);
    let link = a.link();
    let mut stack = stack(a);
    stack.test_insert_neighbor(
        Ipv4Addr::new(B),
        [0x02, 0, 0, 0, 0, 0x02],
        Instant::ZERO,
        600_000,
    );
    stack
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    let trace = link.trace();
    let mut offered = false;
    for entry in trace.entries() {
        let Ok(ethernet) = EthernetFrame::parse(&entry.bytes) else {
            continue;
        };
        if ethernet.ether_type() != EtherType::Ipv4 {
            continue;
        }
        let Ok(ipv4) = Ipv4Packet::parse(ethernet.payload()) else {
            continue;
        };
        if ipv4.protocol() != 6 {
            continue;
        }
        let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
        let Ok(tcp) = TcpPacket::parse(ipv4.payload(), context) else {
            continue;
        };
        if tcp.flags().ece() && tcp.flags().cwr() {
            offered = true;
        }
    }
    assert!(offered, "ECN is offered on the SYN by default");
    assert_eq!(stack.get_property("tcp.ecn_enable"), Ok(1));
}

#[test]
fn accepted_deviation_challenge_ack_limits_are_pinned() {
    let (a, _b) = Link::new(LinkConfig::default(), 10);
    let stack = stack(a);
    assert_eq!(
        stack.get_property("tcp.challenge_ack_global_burst"),
        Ok(1000)
    );
    assert_eq!(
        stack.get_property("tcp.challenge_ack_global_interval"),
        Ok(1000)
    );
    assert_eq!(stack.get_property("tcp.challenge_ack_burst"), Ok(10));
    assert_eq!(stack.get_property("tcp.challenge_ack_interval"), Ok(5000));
}

#[test]
fn measured_initial_window_and_duplicate_ack_threshold_are_pinned() {
    let (a, _b) = Link::new(LinkConfig::default(), 11);
    let stack = stack(a);
    assert_eq!(stack.get_property("tcp.initial_window"), Ok(10));
    assert_eq!(stack.get_property("tcp.dup_ack_threshold"), Ok(3));
    assert_eq!(stack.get_property("tcp.cc_algorithm"), Ok(1));
}

#[test]
fn measured_timewait_and_finwait2_defaults_are_pinned() {
    let (a, _b) = Link::new(LinkConfig::default(), 12);
    let stack = stack(a);
    assert_eq!(stack.get_property("tcp.time_wait_interval"), Ok(60_000));
    assert_eq!(stack.get_property("tcp.finwait2_timeout"), Ok(60_000));
    assert_eq!(stack.get_property("tcp.keepalive_interval"), Ok(7_200_000));
    assert_eq!(stack.get_property("tcp.keepalive_probes"), Ok(9));
}
