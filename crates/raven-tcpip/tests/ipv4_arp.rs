// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ARP, IPv4 input/output, routing, fragmentation and reassembly tests.

#![cfg(feature = "testutil")]

use raven_abi::errno::Errno;
use raven_tcpip::{
    Duration, Instant, Ipv4Addr, Ipv4Config, LoopbackDevice, Route, RouteTable, Stack, StackConfig,
};
use raven_tcpip_wire::arp::ArpBuilder;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{
    Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Flags, Ipv4Header, Ipv4Packet,
};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn new_stack(config: StackConfig, frame_mtu: usize) -> Stack<LoopbackDevice> {
    let device = LoopbackDevice::with_mtu(frame_mtu);
    let mut stack = Stack::new(device, config, 0x1234, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_set_trace(true);
    stack
}

fn default_stack() -> Stack<LoopbackDevice> {
    new_stack(StackConfig::default(), 1514)
}

fn ethernet(dst: [u8; 6], src: [u8; 6], ether_type: EtherType, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0u8; 14 + payload.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(dst),
        MacAddr::new(src),
        ether_type,
        payload.len(),
    )
    .expect("ethernet builder");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    frame
}

fn arp_frame(operation: u16, sha: [u8; 6], spa: [u8; 4], tha: [u8; 6], tpa: [u8; 4]) -> Vec<u8> {
    let mut packet = [0u8; 28];
    let builder = ArpBuilder::new(
        &mut packet,
        operation,
        MacAddr::new(sha),
        WireIpv4Addr::new(spa),
        MacAddr::new(tha),
        WireIpv4Addr::new(tpa),
    )
    .expect("arp builder");
    builder.finish();
    ethernet([0xff; 6], sha, EtherType::Arp, &packet)
}

fn ipv4_frame(
    src: [u8; 4],
    dst: [u8; 4],
    id: u16,
    protocol: u8,
    flags: Ipv4Flags,
    frag_offset: u16,
    payload: &[u8],
) -> Vec<u8> {
    let header = Ipv4Header {
        id,
        flags,
        frag_offset,
        ttl: 64,
        protocol,
        src: WireIpv4Addr::new(src),
        dst: WireIpv4Addr::new(dst),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + payload.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &header, payload.len()).expect("ipv4 builder");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    ethernet(BMAC, AMAC, EtherType::Ipv4, &packet)
}

fn ethernet_ipv4(frame: &[u8]) -> Ipv4Packet<'_> {
    let ethernet = EthernetFrame::parse(frame).expect("ethernet");
    assert_eq!(ethernet.ether_type(), EtherType::Ipv4);
    Ipv4Packet::parse(ethernet.payload()).expect("ipv4")
}

#[test]
fn set_ipv4_installs_a_connected_route() {
    let stack = default_stack();
    assert_eq!(stack.routes().len(), 1);
    assert_eq!(stack.ipv4().expect("configured").addr, address(A));
    assert_eq!(
        stack
            .routes()
            .lookup(address(B))
            .expect("on-link")
            .prefix_len,
        24
    );
}

#[test]
fn set_ipv4_with_gateway_installs_a_default_route() {
    let device = LoopbackDevice::with_mtu(1514);
    let mut stack = Stack::new(device, StackConfig::default(), 1, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(A),
            prefix_len: 24,
            gateway: Some(address([192, 0, 2, 254])),
        })
        .expect("set_ipv4");
    assert_eq!(stack.routes().len(), 2);
    assert_eq!(
        stack
            .routes()
            .lookup(address([8, 8, 8, 8]))
            .expect("default")
            .gateway,
        Some(address([192, 0, 2, 254]))
    );
}

#[test]
fn longest_prefix_wins_over_default() {
    let mut table = RouteTable::new(8);
    table
        .add(Route {
            prefix: address([0, 0, 0, 0]),
            prefix_len: 0,
            gateway: Some(address([192, 0, 2, 254])),
            mtu: 1500,
        })
        .expect("default");
    table
        .add(Route {
            prefix: address([10, 0, 0, 0]),
            prefix_len: 8,
            gateway: None,
            mtu: 1500,
        })
        .expect("10/8");
    table
        .add(Route {
            prefix: address([10, 1, 0, 0]),
            prefix_len: 16,
            gateway: None,
            mtu: 1500,
        })
        .expect("10.1/16");
    assert_eq!(
        table
            .lookup(address([10, 1, 2, 3]))
            .expect("match")
            .prefix_len,
        16
    );
    assert_eq!(
        table
            .lookup(address([10, 2, 2, 3]))
            .expect("match")
            .prefix_len,
        8
    );
    assert_eq!(
        table
            .lookup(address([8, 8, 8, 8]))
            .expect("default")
            .prefix_len,
        0
    );
}

#[test]
fn route_table_is_bounded() {
    let mut table = RouteTable::new(2);
    for index in 0..2u8 {
        table
            .add(Route {
                prefix: address([10, index, 0, 0]),
                prefix_len: 16,
                gateway: None,
                mtu: 1500,
            })
            .expect("route");
    }
    assert_eq!(
        table.add(Route {
            prefix: address([10, 9, 0, 0]),
            prefix_len: 16,
            gateway: None,
            mtu: 1500,
        }),
        Err(Errno::ENOSPC)
    );
}

#[test]
fn no_route_is_enetunreach() {
    let mut stack = default_stack();
    assert_eq!(
        stack.test_send_ipv4(b"x", address([203, 0, 113, 1]), 17),
        Err(Errno::ENETUNREACH)
    );
}

#[test]
fn unresolved_destination_sends_an_arp_request_and_queues_one_packet() {
    let mut stack = default_stack();
    stack
        .test_send_ipv4(b"first", address(B), 17)
        .expect("queue first");
    assert_eq!(stack.test_arp_requests(), 1);
    assert!(
        stack
            .neighbors()
            .get(address(B))
            .expect("entry")
            .has_queued()
    );
    assert_eq!(
        stack.test_send_ipv4(b"second", address(B), 17),
        Err(Errno::EAGAIN)
    );
    assert_eq!(stack.poll(Instant::ZERO).dropped, 1);
}

#[test]
fn arp_reply_resolves_the_neighbour_and_flushes_the_queue() {
    let mut stack = default_stack();
    stack
        .test_send_ipv4(b"payload", address(B), 17)
        .expect("queue");
    stack.test_inject_frame(&arp_frame(2, BMAC, B, AMAC, A), Instant::ZERO);
    assert_eq!(
        stack.neighbors().get(address(B)).expect("entry").mac,
        Some(BMAC)
    );
    stack.poll(Instant::ZERO);
    let frames = stack.test_trace();
    assert!(frames.iter().any(|frame| {
        EthernetFrame::parse(frame).is_ok_and(|ethernet| ethernet.ether_type() == EtherType::Ipv4)
    }));
}

#[test]
fn arp_request_for_our_address_gets_a_reply() {
    let mut stack = default_stack();
    stack.test_inject_frame(&arp_frame(1, BMAC, B, [0; 6], A), Instant::ZERO);
    stack.poll(Instant::ZERO);
    let frames = stack.test_trace();
    let reply = frames
        .iter()
        .find_map(|frame| {
            let ethernet = EthernetFrame::parse(frame).ok()?;
            if ethernet.ether_type() == EtherType::Arp {
                raven_tcpip_wire::arp::ArpPacket::parse(ethernet.payload()).ok()
            } else {
                None
            }
        })
        .expect("ARP reply");
    assert_eq!(reply.operation(), 2);
    assert_eq!(reply.sender_protocol().octets(), A);
    assert_eq!(reply.target_protocol().octets(), B);
}

#[test]
fn unsolicited_arp_does_not_create_an_entry() {
    let mut stack = default_stack();
    stack.test_inject_frame(
        &arp_frame(1, BMAC, B, [0; 6], [192, 0, 2, 99]),
        Instant::ZERO,
    );
    assert!(stack.neighbors().is_empty());
}

#[test]
fn gratuitous_arp_only_refreshes_an_existing_entry() {
    let mut stack = default_stack();
    stack.test_inject_frame(&arp_frame(1, BMAC, B, [0; 6], B), Instant::ZERO);
    assert!(stack.neighbors().get(address(B)).is_none());
    stack.test_insert_neighbor(address(A), BMAC, Instant::ZERO, 60_000);
    stack.test_inject_frame(&arp_frame(1, BMAC, A, [0; 6], A), Instant::from_millis(1));
    assert_eq!(
        stack.neighbors().get(address(A)).expect("entry").mac,
        Some(BMAC)
    );
}

#[test]
fn neighbour_cache_evicts_the_least_recently_used() {
    let config = StackConfig {
        neighbor_cache_entries: 2,
        ..StackConfig::default()
    };
    let mut stack = new_stack(config, 1514);
    stack.test_insert_neighbor(address([192, 0, 2, 10]), [1; 6], Instant::ZERO, 60_000);
    stack.test_insert_neighbor(address([192, 0, 2, 11]), [2; 6], Instant::ZERO, 60_000);
    stack.test_insert_neighbor(address([192, 0, 2, 12]), [3; 6], Instant::ZERO, 60_000);
    assert_eq!(stack.neighbors().len(), 2);
    assert!(stack.neighbors().get(address([192, 0, 2, 10])).is_none());
}

#[test]
fn arp_retries_with_backoff_then_reports_ehostunreach() {
    let mut stack = default_stack();
    stack
        .test_send_ipv4(b"lost", address(B), 17)
        .expect("queue");
    for time in [1000, 3000, 7000, 15_000] {
        stack.poll(Instant::from_millis(time));
    }
    assert!(stack.test_arp_requests() >= 4);
    let errors = stack.test_take_ip_errors();
    assert!(errors.contains(&(address(B), Errno::EHOSTUNREACH)));
}

#[test]
fn packet_to_a_foreign_address_is_dropped() {
    let mut stack = default_stack();
    stack.test_inject_frame(
        &ipv4_frame(B, [192, 0, 2, 99], 1, 17, Ipv4Flags::default(), 0, b"data"),
        Instant::ZERO,
    );
    assert!(stack.test_take_delivered().is_empty());
    assert_eq!(stack.poll(Instant::ZERO).dropped, 1);
}

#[test]
fn bad_header_checksum_is_dropped() {
    let mut stack = default_stack();
    let mut frame = ipv4_frame(B, A, 1, 17, Ipv4Flags::default(), 0, b"data");
    frame[14 + 10] ^= 0xff;
    stack.test_inject_frame(&frame, Instant::ZERO);
    assert!(stack.test_take_delivered().is_empty());
    assert_eq!(stack.poll(Instant::ZERO).dropped, 1);
}

#[test]
fn broadcast_is_delivered_locally() {
    let mut stack = default_stack();
    stack.test_inject_frame(
        &ipv4_frame(
            B,
            [255, 255, 255, 255],
            1,
            17,
            Ipv4Flags::default(),
            0,
            b"broadcast",
        ),
        Instant::ZERO,
    );
    let delivered = stack.test_take_delivered();
    assert_eq!(delivered, vec![(17, b"broadcast".to_vec())]);
}

#[test]
fn transport_payload_is_delivered_with_its_protocol() {
    let mut stack = default_stack();
    stack.test_inject_frame(
        &ipv4_frame(B, A, 7, 1, Ipv4Flags::default(), 0, b"icmp-ish"),
        Instant::ZERO,
    );
    assert_eq!(stack.test_take_delivered(), vec![(1, b"icmp-ish".to_vec())]);
}

#[test]
fn a_4000_byte_payload_fragments_into_three_packets() {
    let mut stack = default_stack();
    stack.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    let payload = vec![0x5au8; 4000];
    stack
        .test_send_ipv4(&payload, address(B), 17)
        .expect("send");
    stack.poll(Instant::ZERO);
    let frames = stack.test_trace();
    assert_eq!(frames.len(), 3);
    let mut lengths = Vec::new();
    let mut offsets = Vec::new();
    for frame in frames {
        let packet = ethernet_ipv4(frame);
        lengths.push(packet.payload_len());
        offsets.push(packet.frag_offset());
    }
    assert_eq!(lengths, vec![1480, 1480, 1040]);
    assert_eq!(offsets, vec![0, 1480, 2960]);
}

#[test]
fn fragments_reassemble_in_reverse_order() {
    let mut stack = default_stack();
    let payload = vec![0x42u8; 3000];
    let first = ipv4_frame(B, A, 9, 17, Ipv4Flags::MF, 0, &payload[..1480]);
    let middle = ipv4_frame(B, A, 9, 17, Ipv4Flags::MF, 1480, &payload[1480..2960]);
    let last = ipv4_frame(B, A, 9, 17, Ipv4Flags::default(), 2960, &payload[2960..]);
    for frame in [&last, &middle, &first] {
        stack.test_inject_frame(frame, Instant::ZERO);
    }
    assert_eq!(stack.test_take_delivered(), vec![(17, payload.clone())]);
    assert_eq!(stack.test_reassembly_completed(), 1);
    assert_eq!(stack.test_reassembly_used(), 0);
}

#[test]
fn a_missing_middle_fragment_times_out_and_frees_memory() {
    let mut stack = default_stack();
    let payload = vec![0x42u8; 3000];
    stack.test_inject_frame(
        &ipv4_frame(B, A, 9, 17, Ipv4Flags::MF, 0, &payload[..1480]),
        Instant::ZERO,
    );
    stack.test_inject_frame(
        &ipv4_frame(B, A, 9, 17, Ipv4Flags::default(), 2960, &payload[2960..]),
        Instant::ZERO,
    );
    assert_eq!(stack.test_reassembly_entries(), 1);
    stack.poll(Instant::from_millis(30_001));
    assert_eq!(stack.test_reassembly_entries(), 0);
    assert_eq!(stack.test_reassembly_used(), 0);
    assert!(stack.test_reassembly_dropped() >= 1);
    assert!(stack.test_take_delivered().is_empty());
}

#[test]
fn overlapping_fragments_discard_the_datagram() {
    let mut stack = default_stack();
    let payload = vec![0x42u8; 3000];
    stack.test_inject_frame(
        &ipv4_frame(B, A, 9, 17, Ipv4Flags::MF, 0, &payload[..1480]),
        Instant::ZERO,
    );
    stack.test_inject_frame(
        &ipv4_frame(B, A, 9, 17, Ipv4Flags::MF, 0, &payload[..100]),
        Instant::ZERO,
    );
    assert_eq!(stack.test_reassembly_entries(), 0);
    assert!(stack.test_reassembly_dropped() >= 1);
    assert!(stack.test_take_delivered().is_empty());
}

#[test]
fn reassembly_bytes_exhaustion_drops_new_keys_without_allocating() {
    let config = StackConfig {
        reassembly_bytes: 100,
        ..StackConfig::default()
    };
    let mut stack = new_stack(config, 1514);
    let payload = vec![0u8; 80];
    stack.test_inject_frame(
        &ipv4_frame(B, A, 1, 17, Ipv4Flags::MF, 0, &payload),
        Instant::ZERO,
    );
    assert_eq!(stack.test_reassembly_used(), 80);
    stack.test_inject_frame(
        &ipv4_frame(B, A, 2, 17, Ipv4Flags::MF, 0, &payload),
        Instant::ZERO,
    );
    assert_eq!(stack.test_reassembly_used(), 80);
    assert_eq!(stack.test_reassembly_entries(), 1);
    assert!(stack.test_reassembly_dropped() >= 1);
}

#[test]
fn one_hour_forward_does_not_disturb_a_completed_reassembly() {
    let mut stack = default_stack();
    stack.test_inject_frame(
        &ipv4_frame(B, A, 1, 17, Ipv4Flags::default(), 0, b"whole"),
        Instant::ZERO,
    );
    stack.poll(Instant::from_millis(3_600_000));
    assert_eq!(stack.test_take_delivered(), vec![(17, b"whole".to_vec())]);
}

#[test]
fn thousand_pseudo_random_fragment_interleavings_never_leak() {
    let mut stack = default_stack();
    let payload = vec![0xa5u8; 3000];
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    for round in 0..1000u16 {
        let mut order = [0u8, 1, 2];
        for index in (1..3).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let swap = usize::try_from(state % (index as u64 + 1)).expect("index");
            order.swap(index, swap);
        }
        for piece in order {
            let frame = match piece {
                0 => ipv4_frame(B, A, round, 17, Ipv4Flags::MF, 0, &payload[..1480]),
                1 => ipv4_frame(B, A, round, 17, Ipv4Flags::MF, 1480, &payload[1480..2960]),
                _ => ipv4_frame(
                    B,
                    A,
                    round,
                    17,
                    Ipv4Flags::default(),
                    2960,
                    &payload[2960..],
                ),
            };
            stack.test_inject_frame(&frame, Instant::from_millis(i64::from(round)));
        }
        assert_eq!(stack.test_reassembly_used(), 0, "round {round} leaked");
        assert_eq!(
            stack.test_take_delivered(),
            vec![(17, payload.clone())],
            "round {round}"
        );
    }
    assert_eq!(stack.test_reassembly_completed(), 1000);
}

#[test]
fn reassembly_timeout_constant_is_thirty_seconds() {
    // Pinned by the ticket and mirrored in `Reassembly::new`.
    assert_eq!(Duration::from_millis(30_000).as_millis(), 30_000);
}
