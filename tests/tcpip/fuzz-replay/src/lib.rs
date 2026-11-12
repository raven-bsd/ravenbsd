// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable replay harness shared by the nightly fuzz targets and the ordinary
//! test suite. Every function is deterministic and runs on stable Rust.

#![forbid(unsafe_code)]

use raven_tcpip::{
    ChecksumOffload, Device, DeviceCaps, Instant, IpAddr, Ipv4Addr, Ipv4Config, Medium, SocketId,
    Stack, StackConfig, TxError,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{
    Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Flags, Ipv4Header, Ipv4Packet,
};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpOptions, TcpPacket};
use raven_tcpip_wire::{ArpPacket, Icmpv4Packet, Icmpv6Packet, Ipv6Packet, UdpPacket};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn context4(data: &[u8]) -> ChecksumContext {
    let mut src = [0u8; 4];
    let mut dst = [0u8; 4];
    for (index, slot) in src.iter_mut().enumerate() {
        *slot = data.get(index).copied().unwrap_or(0);
    }
    for (index, slot) in dst.iter_mut().enumerate() {
        *slot = data.get(index + 4).copied().unwrap_or(0);
    }
    ChecksumContext::ipv4(WireIpv4Addr::new(src), WireIpv4Addr::new(dst))
}

fn context6(data: &[u8]) -> ChecksumContext {
    let mut src = [0u8; 16];
    let mut dst = [0u8; 16];
    for (index, slot) in src.iter_mut().enumerate() {
        *slot = data.get(index).copied().unwrap_or(0);
    }
    for (index, slot) in dst.iter_mut().enumerate() {
        *slot = data.get(index + 16).copied().unwrap_or(0);
    }
    ChecksumContext::ipv6(
        raven_tcpip_wire::ipv6::Ipv6Addr::new(src),
        raven_tcpip_wire::ipv6::Ipv6Addr::new(dst),
    )
}

/// Parse `data` with every wire parser. Total function: it may error, never panic.
pub fn wire_parse(data: &[u8]) {
    let _ = EthernetFrame::parse(data);
    let _ = ArpPacket::parse(data);
    let _ = Ipv4Packet::parse(data);
    let _ = Ipv6Packet::parse(data);
    let _ = Icmpv4Packet::parse(data);
    let _ = Icmpv6Packet::parse(data, context6(data));
    let context = context4(data);
    let _ = TcpPacket::parse(data, context);
    let _ = UdpPacket::parse(data, context);
    let context = context6(data);
    let _ = TcpPacket::parse(data, context);
    let _ = UdpPacket::parse(data, context);
}

/// Parse arbitrary TCP option bytes; if they are re-serialisable, the
/// re-serialised packet must yield the same option kinds.
pub fn tcp_options(data: &[u8]) {
    let mut kinds = Vec::new();
    for option in TcpOptions::new(data).take(data.len().saturating_add(1)) {
        match option {
            Ok(option) => kinds.push(option_kind(option)),
            Err(_) => return,
        }
    }
    let header = TcpHeader {
        src_port: 1,
        dst_port: 2,
        sequence: 1,
        acknowledgement: 2,
        flags: TcpFlags::ACK,
        window: 4096,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(A), WireIpv4Addr::new(B));
    let mut bytes = vec![0u8; 20 + data.len().div_ceil(4) * 4];
    let Ok(builder) = TcpBuilder::new(&mut bytes, context, &header, data, 0) else {
        return;
    };
    builder.finish();
    let Ok(packet) = TcpPacket::parse(&bytes, context) else {
        return;
    };
    let mut again = Vec::new();
    for option in packet.options().take(data.len().saturating_add(1)) {
        match option {
            Ok(option) => again.push(option_kind(option)),
            Err(_) => return,
        }
    }
    while kinds.last() == Some(&0) {
        kinds.pop();
    }
    while again.last() == Some(&0) {
        again.pop();
    }
    assert_eq!(kinds, again, "option round trip changed the parsed kinds");
}

fn option_kind(option: TcpOption<'_>) -> u8 {
    match option {
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
    }
}

fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(LinkConfig::default(), 91);
    let link = a.link();
    let mut a = Stack::new(a, StackConfig::default(), 1, Instant::ZERO);
    let mut b = Stack::new(b, StackConfig::default(), 2, Instant::ZERO);
    for (stack, address) in [(&mut a, A), (&mut b, B)] {
        stack
            .set_ipv4(Ipv4Config {
                addr: Ipv4Addr::new(address),
                prefix_len: 24,
                gateway: None,
            })
            .expect("ipv4");
    }
    a.test_insert_neighbor(Ipv4Addr::new(B), BMAC, Instant::ZERO, 600_000);
    b.test_insert_neighbor(Ipv4Addr::new(A), AMAC, Instant::ZERO, 600_000);
    (link, a, b)
}

fn handshake(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>) -> (SocketId, SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let client = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    for time in [0, 50, 100, 150, 200] {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let server = b
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    (client, server)
}

#[allow(clippy::too_many_arguments)]
fn build_segment(
    src: [u8; 4],
    dst: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    payload: &[u8],
) -> Vec<u8> {
    let header = TcpHeader {
        src_port,
        dst_port,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window: 65_535,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(src), WireIpv4Addr::new(dst));
    let mut segment = vec![0u8; 20 + payload.len()];
    let mut builder =
        TcpBuilder::new(&mut segment, context, &header, &[], payload.len()).expect("tcp");
    builder.payload_mut().copy_from_slice(payload);
    builder.finish();
    let ip_header = Ipv4Header {
        id: 7,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(src),
        dst: WireIpv4Addr::new(dst),
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

/// Drive arbitrary payload segments into an established connection. The
/// receive stream must be a prefix of the bytes the peer legitimately queued,
/// counters must not go backwards, and queues stay under their caps.
pub fn segment_sequence(data: &[u8]) {
    let (_link, mut a, mut b) = pair();
    let (client, _server) = handshake(&mut a, &mut b);
    a.test_set_trace(true);
    let mut expected = Vec::new();
    let mut received_stream = Vec::new();
    let mut previous: Vec<u64> = raven_tcpip::Counters::names()
        .iter()
        .map(|name| a.counters().get(name).unwrap_or(0))
        .collect();
    let mut time = 210i64;
    let mut cursor = 0usize;
    while cursor < data.len() {
        let length = usize::from(data[cursor]) % 32;
        cursor += 1;
        let flags = data.get(cursor).copied().unwrap_or(0);
        cursor += 1;
        let end = cursor.saturating_add(length).min(data.len());
        if cursor > data.len() {
            break;
        }
        let payload = &data[cursor..end];
        cursor = end;
        let seq = a.test_tcp_rcv_nxt(client).expect("rcv_nxt");
        let (_, _, snd_nxt) = a.test_tcp_window(client).expect("window");
        let frame = build_segment(
            B,
            A,
            80,
            40_000,
            seq,
            snd_nxt,
            {
                let mut bits = TcpFlags::ACK.bits();
                if flags & 0x08 != 0 {
                    bits |= TcpFlags::PSH.bits();
                }
                TcpFlags::from_bits(bits)
            },
            payload,
        );
        a.test_inject_frame(&frame, Instant::from_millis(time));
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        expected.extend_from_slice(payload);
        let mut buffer = vec![0u8; 4096];
        while let Ok(count) = a.recv(client, &mut buffer) {
            if count == 0 {
                break;
            }
            received_stream.extend_from_slice(&buffer[..count]);
        }
        assert_eq!(
            received_stream, expected,
            "receive stream is not the peer stream (or has fabricated bytes)"
        );
        let current: Vec<u64> = raven_tcpip::Counters::names()
            .iter()
            .map(|name| a.counters().get(name).unwrap_or(0))
            .collect();
        for (index, value) in current.iter().enumerate() {
            assert!(*value >= previous[index], "counter went backwards");
        }
        previous = current;
        assert!(a.recv_queue_len(client).unwrap_or(0) <= 64 * 1024);
        assert!(a.send_queue_len(client).unwrap_or(0) <= 64 * 1024);
        time = time.saturating_add(5);
    }
}

/// Arbitrary segments from Closed. States must stay legal and Established
/// requires a SYN and an ACK to have been injected.
pub fn handshake_state(data: &[u8]) {
    let (_link, mut stack, _peer) = pair();
    stack.tcp_listen(80, 8).expect("listen");
    let mut syn_seen = false;
    let mut ack_seen = false;
    let mut time = 0i64;
    for chunk in data.chunks(8) {
        let flags = TcpFlags::from_bits(chunk.first().copied().unwrap_or(0) & 0x3f);
        let seq = u32::from(chunk.get(1).copied().unwrap_or(0));
        let ack = u32::from(chunk.get(2).copied().unwrap_or(0));
        syn_seen |= flags.syn();
        ack_seen |= flags.ack();
        let frame = build_segment(B, A, 40_000, 80, 1000u32.wrapping_add(seq), ack, flags, &[]);
        stack.test_inject_frame(&frame, Instant::from_millis(time));
        stack.poll(Instant::from_millis(time));
        for socket in stack.sockets().collect::<Vec<_>>() {
            if let Ok(state) = stack.tcp_state(socket) {
                let _ = state;
            }
        }
        time = time.saturating_add(10);
        for socket in stack.sockets().collect::<Vec<_>>() {
            if stack.tcp_state(socket) == Ok(raven_tcpip::TcpState::Established) {
                assert!(syn_seen && ack_seen, "Established without a SYN and ACK");
            }
        }
    }
}

/// Arbitrary IPv4 fragments. The pool must return to its baseline after expiry.
pub fn reassembly(data: &[u8]) {
    let mut stack = Stack::new(NoDevice, StackConfig::default(), 3, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("ipv4");
    let mut cursor = 0usize;
    while cursor < data.len() {
        let id = u16::from(data[cursor]);
        cursor += 1;
        let offset = u16::from(data.get(cursor).copied().unwrap_or(0)) * 8;
        cursor += 1;
        let more = data.get(cursor).copied().unwrap_or(0) & 1 != 0;
        cursor += 1;
        let length = usize::from(data.get(cursor).copied().unwrap_or(0) % 32);
        cursor += 1;
        let end = cursor.saturating_add(length).min(data.len());
        if cursor > data.len() {
            break;
        }
        let payload = &data[cursor..end];
        cursor = end;
        let frame = build_fragment(A, B, id, offset, more, payload);
        stack.test_inject_frame(&frame, Instant::ZERO);
        stack.poll(Instant::from_millis(0));
    }
    stack.poll(Instant::from_millis(120_000));
    assert_eq!(stack.test_reassembly_entries(), 0);
    assert_eq!(stack.test_reassembly_used(), 0);
}

#[allow(clippy::too_many_arguments)]
fn build_fragment(
    src: [u8; 4],
    dst: [u8; 4],
    id: u16,
    offset: u16,
    more: bool,
    payload: &[u8],
) -> Vec<u8> {
    let ip_header = Ipv4Header {
        id,
        ttl: 64,
        protocol: 17,
        flags: if more {
            Ipv4Flags::MF
        } else {
            Ipv4Flags::default()
        },
        frag_offset: offset,
        src: WireIpv4Addr::new(src),
        dst: WireIpv4Addr::new(dst),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + payload.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, payload.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(payload);
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

struct NoDevice;

impl Device for NoDevice {
    fn capabilities(&self) -> DeviceCaps {
        DeviceCaps {
            medium: Medium::Ip,
            mtu: 1500,
            hardware_addr: None,
            checksum_offload: ChecksumOffload::None,
        }
    }
    fn receive(&mut self, _now: Instant, _consume: &mut dyn FnMut(&[u8])) -> bool {
        false
    }
    fn transmit(
        &mut self,
        _now: Instant,
        _len: usize,
        _fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        Ok(())
    }
}

/// Replay one corpus file through the named target.
pub fn replay(target: &str, data: &[u8]) -> Result<(), String> {
    match target {
        "wire_parse" => wire_parse(data),
        "tcp_options" => tcp_options(data),
        "segment_sequence" => segment_sequence(data),
        "handshake_state" => handshake_state(data),
        "reassembly" => reassembly(data),
        other => return Err(format!("unknown fuzz target {other}")),
    }
    Ok(())
}

/// The five target names.
pub const TARGETS: [&str; 5] = [
    "wire_parse",
    "tcp_options",
    "segment_sequence",
    "handshake_state",
    "reassembly",
];
