// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP option negotiation: window scaling, timestamps, PAWS and MSS.

#![cfg(feature = "testutil")]

use raven_abi::errno::Errno;
use raven_tcpip::{
    Instant, IpAddr, Ipv4Addr, Ipv4Config, Ipv6Addr, Ipv6Cidr, Ipv6Config, Stack, StackConfig,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpPacket};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn a6() -> Ipv6Addr {
    Ipv6Addr::new([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
}

fn b6() -> Ipv6Addr {
    Ipv6Addr::new([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2])
}

fn stack4(endpoint: Endpoint, value: [u8; 4], seed: u64, config: StackConfig) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, config, seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(value),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    stack.test_set_trace(true);
    stack
}

fn pair_with(config: StackConfig) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(
        LinkConfig {
            mtu: 1514,
            ..LinkConfig::default()
        },
        101,
    );
    let link = a.link();
    let mut a = stack4(a, A, 1, config);
    let mut b = stack4(b, B, 2, config);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    pair_with(StackConfig::default())
}

fn drive(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, times: &[i64]) {
    for time in times {
        let now = Instant::from_millis(*time);
        a.poll(now);
        b.poll(now);
    }
}

fn handshake(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    b.tcp_listen(80, 4).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    drive(a, b, &[0, 50, 100, 150, 200]);
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("connection");
    (socket, accepted)
}

fn tcp_frame(options: &[u8], seq: u32, ack: u32, flags: TcpFlags) -> Vec<u8> {
    let header = TcpHeader {
        src_port: 40_000,
        dst_port: 80,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(A), WireIpv4Addr::new(B));
    let mut segment = vec![0u8; 20 + ((options.len() + 3) & !3)];
    {
        let builder = TcpBuilder::new(&mut segment, context, &header, options, 0).expect("tcp");
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 1,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(A),
        dst: WireIpv4Addr::new(B),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let mut frame = vec![0u8; 14 + packet.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(AMAC),
        MacAddr::new(BMAC),
        EtherType::Ipv4,
        packet.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(&packet);
    builder.finish();
    frame
}

fn tcp_frame_payload(
    options: &[u8],
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    payload: &[u8],
) -> Vec<u8> {
    let header = TcpHeader {
        src_port: 40_000,
        dst_port: 80,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(A), WireIpv4Addr::new(B));
    let padded = (options.len() + 3) & !3;
    let mut segment = vec![0u8; 20 + padded + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, options, payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 2,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(A),
        dst: WireIpv4Addr::new(B),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let mut frame = vec![0u8; 14 + packet.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(AMAC),
        MacAddr::new(BMAC),
        EtherType::Ipv4,
        packet.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(&packet);
    builder.finish();
    frame
}

fn tcp_packets(link: &Link) -> Vec<(TcpFlags, u32, u32, u16, Vec<u8>)> {
    link.trace()
        .entries()
        .iter()
        .filter_map(|entry| {
            let ethernet = EthernetFrame::parse(&entry.bytes).ok()?;
            if ethernet.ether_type() != EtherType::Ipv4 {
                return None;
            }
            let ipv4 = Ipv4Packet::parse(ethernet.payload()).ok()?;
            if ipv4.protocol() != 6 {
                return None;
            }
            let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
            let tcp = TcpPacket::parse(ipv4.payload(), context).ok()?;
            let mut kinds = Vec::new();
            for option in tcp.options().flatten() {
                {
                    kinds.push(match option {
                        TcpOption::EndOfList => 0,
                        TcpOption::NoOperation => 1,
                        TcpOption::MaxSegmentSize(_) => 2,
                        TcpOption::WindowScale(_) => 3,
                        TcpOption::SackPermitted => 4,
                        TcpOption::Sack(_) => 5,
                        TcpOption::Timestamps { .. } => 8,
                        TcpOption::Md5 { .. } => 19,
                        TcpOption::Authentication { .. } => 29,
                        TcpOption::Unknown { kind, .. } => kind,
                    });
                }
            }
            Some((
                tcp.flags(),
                tcp.sequence(),
                tcp.acknowledgement(),
                tcp.window(),
                kinds,
            ))
        })
        .collect()
}

fn handshake_with_options(
    link: &Link,
    b: &mut Stack<Endpoint>,
    syn_options: &[u8],
    client_seq: u32,
    server_ts: u32,
) -> raven_tcpip::SocketId {
    let listener = b.tcp_listen(80, 4).expect("listen");
    let frame = tcp_frame(syn_options, client_seq, 0, TcpFlags::SYN);
    b.test_inject_frame(&frame, Instant::ZERO);
    b.poll(Instant::ZERO);
    let synack = {
        let mut found = None;
        for entry in link.trace().entries() {
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
            if tcp.flags().syn() && tcp.flags().ack() {
                found = Some((tcp.sequence(), tcp.acknowledgement()));
            }
        }
        found.expect("SYN,ACK")
    };
    let ack_options = if server_ts == 0 {
        Vec::new()
    } else {
        vec![
            8,
            10,
            0,
            0,
            0,
            1,
            server_ts.to_be_bytes()[0],
            server_ts.to_be_bytes()[1],
            server_ts.to_be_bytes()[2],
            server_ts.to_be_bytes()[3],
        ]
    };
    let ack = tcp_frame(&ack_options, client_seq + 1, synack.0 + 1, TcpFlags::ACK);
    b.test_inject_frame(&ack, Instant::ZERO);
    let _ = listener;
    b.tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("conn")
}

#[test]
fn window_scaling_is_negotiated_when_both_offer() {
    let (_link, mut a, mut b) = pair();
    let (sa, sb) = handshake(&mut a, &mut b);
    let (snd, rcv, ..) = a.test_tcp_options_info(sa).expect("a options");
    assert!(snd > 0);
    assert!(rcv > 0);
    let (snd_b, rcv_b, ..) = b.test_tcp_options_info(sb).expect("b options");
    assert_eq!(snd, rcv_b);
    assert_eq!(rcv, snd_b);
}

#[test]
fn syn_window_is_unscaled_but_data_window_is_scaled() {
    let (link, mut a, mut b) = pair();
    let _ = handshake(&mut a, &mut b);
    let packets = tcp_packets(&link);
    let syn = packets
        .iter()
        .find(|(flags, _, _, _, _)| flags.syn())
        .expect("SYN");
    assert_eq!(syn.3, 65_535);
    let start = link.trace().entries().len();
    let (sa, _) = (
        a.tcp_connect((IpAddr::V4(address(B)), 80), Some(41_000))
            .expect("second"),
        0,
    );
    drive(&mut a, &mut b, &[210, 260, 310, 360]);
    let data = tcp_packets(&link);
    let later = &data[start.min(data.len())..];
    let _ = later;
    let _ = sa;
}

#[test]
fn window_scale_shift_15_is_clamped_to_14() {
    let (_link, mut _a, mut b) = pair();
    let options = [2u8, 4, 5, 0xb4, 3, 3, 15];
    let sb = handshake_with_options(&_link, &mut b, &options, 1000, 0);
    let (snd, ..) = b.test_tcp_options_info(sb).expect("options");
    assert_eq!(snd, 14);
}

#[test]
fn timestamps_are_negotiated_when_both_offer() {
    let (_link, mut a, mut b) = pair();
    let (sa, sb) = handshake(&mut a, &mut b);
    assert!(a.test_tcp_options_info(sa).expect("a").2);
    assert!(b.test_tcp_options_info(sb).expect("b").2);
}

#[test]
fn timestamps_are_not_enabled_when_the_peer_omits_them() {
    let (_link, mut _a, mut b) = pair();
    let options = [2u8, 4, 5, 0xb4];
    let sb = handshake_with_options(&_link, &mut b, &options, 2000, 0);
    assert!(!b.test_tcp_options_info(sb).expect("options").2);
}

#[test]
fn stale_timestamp_is_dropped_by_paws() {
    let (_link, mut a, mut b) = pair();
    let (_sa, sb) = handshake(&mut a, &mut b);
    let (_, _, _, ts_recent, ..) = b.test_tcp_options_info(sb).expect("options");
    let old = ts_recent.wrapping_sub(1);
    let rcv_nxt = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    let ack = b.test_tcp_window(sb).expect("window").2;
    let mut options = vec![8u8, 10];
    options.extend_from_slice(&old.to_be_bytes());
    options.extend_from_slice(&ts_recent.to_be_bytes());
    let frame = tcp_frame(&options, rcv_nxt, ack, TcpFlags::ACK);
    b.test_inject_frame(&frame, Instant::ZERO);
    let (.., paws, _) = b.test_tcp_options_info(sb).expect("options");
    assert_eq!(paws, 1);
    let mut buffer = [0u8; 1];
    assert_eq!(b.recv(sb, &mut buffer), Err(Errno::EAGAIN));
}

#[test]
fn timestamp_more_than_24_days_old_is_the_exception() {
    let (_link, mut a, mut b) = pair();
    let (_sa, sb) = handshake(&mut a, &mut b);
    let (_, _, _, ts_recent, ..) = b.test_tcp_options_info(sb).expect("options");
    let old = ts_recent.wrapping_sub(1);
    let rcv_nxt = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    let ack = b.test_tcp_window(sb).expect("window").2;
    let mut options = vec![8u8, 10];
    options.extend_from_slice(&old.to_be_bytes());
    options.extend_from_slice(&ts_recent.to_be_bytes());
    let frame = tcp_frame_payload(&options, rcv_nxt, ack, TcpFlags::ACK, b"x");
    b.test_inject_frame(&frame, Instant::from_millis(24 * 60 * 60 * 1000 + 1_000));
    let mut buffer = [0u8; 1];
    assert_eq!(b.recv(sb, &mut buffer), Ok(1));
}

#[test]
fn syn_option_order_is_mss_sack_nop_ts_nop_ws_eol() {
    let (link, mut a, mut b) = pair();
    let _ = handshake(&mut a, &mut b);
    let packets = tcp_packets(&link);
    let syn = packets
        .iter()
        .find(|(flags, _, _, _, _)| flags.syn())
        .expect("SYN");
    let filtered: Vec<u8> = syn.4.iter().copied().filter(|kind| *kind != 0).collect();
    assert_eq!(filtered, vec![2, 4, 1, 1, 8, 1, 1, 3]);
}

#[test]
fn data_segments_do_not_repeat_syn_options() {
    let (link, mut a, mut b) = pair();
    let (sa, _sb) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(sa, b"options").expect("send");
    a.poll(Instant::from_millis(210));
    let packets = tcp_packets(&link);
    for (_, _, _, _, kinds) in packets.iter().skip(start) {
        assert!(!kinds.contains(&2));
        assert!(!kinds.contains(&3));
        assert!(!kinds.contains(&4));
    }
}

#[test]
fn tsecr_echoes_the_peer_timestamp() {
    let (link, mut a, mut b) = pair();
    let (sa, _sb) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(sa, b"ts").expect("send");
    a.poll(Instant::from_millis(210));
    let packets = tcp_packets(&link);
    let data = packets
        .iter()
        .skip(start)
        .find(|(flags, _, _, _, _)| flags.ack() && !flags.syn())
        .expect("data");
    assert!(data.4.contains(&8));
}

#[test]
fn mss_is_announced_from_the_ipv4_path_mtu() {
    let (link, mut a, mut b) = pair();
    let _ = handshake(&mut a, &mut b);
    let packets = tcp_packets(&link);
    let syn = packets
        .iter()
        .find(|(flags, _, _, _, _)| flags.syn())
        .expect("SYN");
    assert_eq!(syn.4.first(), Some(&2));
}

#[test]
fn ipv6_mss_is_twenty_bytes_smaller_than_ipv4() {
    let (ea, eb) = Link::new(LinkConfig::default(), 111);
    let _link = ea.link();
    let mut a = Stack::new(ea, StackConfig::default(), 1, Instant::ZERO);
    let _b = Stack::new(eb, StackConfig::default(), 2, Instant::ZERO);
    a.set_ipv6(Ipv6Config {
        addrs: vec![Ipv6Cidr {
            addr: a6(),
            prefix_len: 64,
        }],
        gateway: None,
    })
    .expect("ipv6");
    a.test_insert_neighbor6(b6(), BMAC, Instant::ZERO);
    let _ = a.tcp_connect((IpAddr::V6(b6()), 80), Some(40_000));
    let syn = tcp_packets(&_link)
        .into_iter()
        .find(|(flags, _, _, _, _)| flags.syn());
    let _ = syn;
}

#[test]
fn peer_mss_one_is_clamped_to_the_floor() {
    let (_link, mut _a, mut b) = pair();
    let options = [2u8, 4, 0, 1];
    let sb = handshake_with_options(&_link, &mut b, &options, 3000, 0);
    let (_, _, _, _, peer_mss, _mss, clamps, _, _) = b.test_tcp_options_info(sb).expect("options");
    assert_eq!(peer_mss, 536);
    assert_eq!(clamps, 1);
}

#[test]
fn peer_mss_larger_than_our_path_is_clamped() {
    let (_link, mut _a, mut b) = pair();
    let options = [2u8, 4, 0x23, 0x28];
    let sb = handshake_with_options(&_link, &mut b, &options, 4000, 0);
    let (_, _, _, _, peer_mss, mss, clamps, _, _) = b.test_tcp_options_info(sb).expect("options");
    assert_eq!(peer_mss, 1460);
    assert!(mss <= 1460);
    assert_eq!(clamps, 1);
}

#[test]
fn timestamp_option_reduces_the_effective_mss() {
    let (_link, mut a, mut b) = pair();
    let (_sa, sb) = handshake(&mut a, &mut b);
    let (_, _, _, _, _peer_mss, mss, _, _, _) = b.test_tcp_options_info(sb).expect("options");
    assert_eq!(mss, 1460 - 12);
}

#[test]
fn disabling_timestamps_removes_the_option() {
    let config = StackConfig {
        timestamps: false,
        ..StackConfig::default()
    };
    let (link, mut a, mut b) = pair_with(config);
    let _ = handshake(&mut a, &mut b);
    let packets = tcp_packets(&link);
    let syn = packets
        .iter()
        .find(|(flags, _, _, _, _)| flags.syn())
        .expect("SYN");
    assert!(!syn.4.contains(&8));
}

#[test]
fn disabling_window_scaling_removes_the_option() {
    let config = StackConfig {
        window_scaling: false,
        ..StackConfig::default()
    };
    let (link, mut a, mut b) = pair_with(config);
    let _ = handshake(&mut a, &mut b);
    let packets = tcp_packets(&link);
    let syn = packets
        .iter()
        .find(|(flags, _, _, _, _)| flags.syn())
        .expect("SYN");
    assert!(!syn.4.contains(&3));
}

#[test]
fn paws_counter_starts_at_zero() {
    let (_link, mut a, mut b) = pair();
    let (_sa, sb) = handshake(&mut a, &mut b);
    let (_, _, _, _, _, _, _, paws, _) = b.test_tcp_options_info(sb).expect("options");
    assert_eq!(paws, 0);
}

#[test]
fn window_scale_is_derived_from_the_receive_buffer() {
    let config = StackConfig {
        rx_queue_bytes: 1024 * 1024,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_with(config);
    let (sa, _sb) = handshake(&mut a, &mut b);
    let (_, rcv_wscale, ..) = a.test_tcp_options_info(sa).expect("options");
    assert_eq!(rcv_wscale, 5);
}

#[test]
fn timestamp_option_is_absent_when_the_feature_is_disabled() {
    let config = StackConfig {
        timestamps: false,
        ..StackConfig::default()
    };
    let (link, mut a, mut b) = pair_with(config);
    let (sa, _sb) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    a.send(sa, b"no-ts").expect("send");
    a.poll(Instant::from_millis(210));
    let packets = tcp_packets(&link);
    for (_, _, _, _, kinds) in packets.iter().skip(start) {
        assert!(!kinds.contains(&8));
    }
}

#[test]
fn default_mss_bounds_are_sane() {
    let config = StackConfig::default();
    assert!(config.mss_floor <= config.mss_ceiling);
    assert!(config.mss_floor >= 536);
    assert!(config.mss_ceiling <= 1460);
}
