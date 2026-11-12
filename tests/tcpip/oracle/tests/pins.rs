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

fn lossy_transfer_ms(loss_percent: f32, reorder_percent: f32) -> i64 {
    let (a, b) = Link::new(
        LinkConfig {
            rtt_ms: 20,
            loss_percent,
            reorder_percent,
            ..LinkConfig::default()
        },
        131,
    );
    let mut client = stack(a);
    let mut server = Stack::new(b, StackConfig::default(), 2, Instant::ZERO);
    server
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(B),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    client.test_insert_neighbor(
        Ipv4Addr::new(B),
        [0x02, 0, 0, 0, 0, 2],
        Instant::ZERO,
        600_000,
    );
    server.test_insert_neighbor(
        Ipv4Addr::new(A),
        [0x02, 0, 0, 0, 0, 1],
        Instant::ZERO,
        600_000,
    );
    server.tcp_listen(80, 8).expect("listen");
    let socket = client
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    let mut now = 0i64;
    let peer = loop {
        if let Ok(Some(peer)) = server.tcp_accept(raven_tcpip::SocketId(1)) {
            break peer;
        }
        now += 1;
        assert!(now < 60_000, "handshake did not finish");
        client.poll(Instant::from_millis(now));
        server.poll(Instant::from_millis(now));
    };
    let start = now;
    let payload = vec![0x42u8; 1 << 20];
    let (mut sent, mut received) = (0usize, 0usize);
    let mut buffer = vec![0u8; 65_536];
    while received < payload.len() {
        if sent < payload.len()
            && let Ok(count) = client.send(socket, &payload[sent..])
        {
            sent += count;
        }
        while let Ok(count) = server.recv(peer, &mut buffer) {
            if count == 0 {
                break;
            }
            received += count;
        }
        now += 1;
        assert!(now - start < 120_000, "transfer stalled at {received}");
        client.poll(Instant::from_millis(now));
        server.poll(Instant::from_millis(now));
    }
    now - start
}

/// C23-D01, re-classified. The recovery defect is fixed (selective retransmission, reordering
/// tolerance with spurious-retransmit undo, RTT sampling). What remains of the gap to the
/// frozen no-congestion-control reference at 1% loss is congestion control: the native stack
/// reduces its window on every loss event, and that reference never did. Against smoltcp with
/// its Reno controller (the fair baseline, 10181 ms) the native stack is about ten times
/// faster. A loss-based sender at 20 ms RTT and 1% loss is bounded near 0.8 MiB/s (Mathis estimate); the pin keeps the
/// native result in that range and far from the pre-C28 803 s.
#[test]
fn c23_d01_loss_goodput_is_in_the_congestion_controlled_range() {
    let elapsed = lossy_transfer_ms(1.0, 0.0);
    assert!(
        (600..2_500).contains(&elapsed),
        "1 MiB at 1% loss and 20 ms RTT took {elapsed} ms"
    );
}

/// The same defect class for reordering: without loss the transfer must not be slowed by
/// spurious fast retransmits, so it stays within a small multiple of the lossless time.
#[test]
fn c23_d01_reordering_does_not_collapse_the_window() {
    let elapsed = lossy_transfer_ms(0.0, 10.0);
    assert!(elapsed < 3_000, "1 MiB at 10% reordering took {elapsed} ms");
}
