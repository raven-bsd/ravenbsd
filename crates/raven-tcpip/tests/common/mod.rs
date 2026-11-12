// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the C19 observability tests.

#![allow(dead_code)]

use raven_abi::errno::Errno;
use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpPacket};

pub const A: [u8; 4] = [192, 0, 2, 1];
pub const B: [u8; 4] = [192, 0, 2, 2];
pub const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
pub const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

pub fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

pub fn endpoint_stack(
    endpoint: Endpoint,
    address_value: [u8; 4],
    seed: u64,
    config: StackConfig,
) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, config, seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(address_value),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_set_trace(true);
    stack
}

pub fn pair_with(config: StackConfig) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    pair_with_link(LinkConfig::default(), config)
}

pub fn pair_with_link(
    link_config: LinkConfig,
    config: StackConfig,
) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(link_config, 71);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1, config);
    let mut b = endpoint_stack(b, B, 2, config);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

pub fn pair() -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    pair_with(StackConfig::default())
}

pub fn handshake(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>) -> (SocketId, SocketId) {
    handshake_with_backlog(a, b, 8)
}

pub fn handshake_with_backlog(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    backlog: usize,
) -> (SocketId, SocketId) {
    b.tcp_listen(80, backlog).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump_ms(a, b, 0, 200);
    let accepted = b
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    (socket, accepted)
}

pub fn pump(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, start: i64, steps: usize) {
    for step in 0..steps {
        let now = Instant::from_millis(start + i64::try_from(step).unwrap_or(0) * 5);
        a.poll(now);
        b.poll(now);
    }
}

pub fn pump_ms(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, start: i64, end: i64) {
    let mut time = start;
    while time <= end {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        time += 5;
    }
}

pub fn send_all(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    send_socket: SocketId,
    recv_socket: SocketId,
    data: &[u8],
    start: i64,
) -> Vec<u8> {
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut time = start;
    let mut buffer = vec![0u8; 4096];
    for _ in 0..20_000 {
        if sent < data.len()
            && let Ok(count) = a.send(send_socket, &data[sent..])
        {
            sent += count;
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        loop {
            match b.recv(recv_socket, &mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    received.extend_from_slice(&buffer[..count]);
                    if received.len() >= data.len() {
                        break;
                    }
                }
                Err(Errno::EAGAIN) => break,
                Err(_) => break,
            }
        }
        if sent == data.len() && received.len() >= data.len() {
            break;
        }
        time += 5;
    }
    received
}

pub fn trace_link_tcp(link: &Link, start: usize) -> Vec<(TcpFlags, u32, u32, u16)> {
    link.trace()
        .entries()
        .iter()
        .skip(start)
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
            Some((
                tcp.flags(),
                tcp.sequence(),
                tcp.acknowledgement(),
                tcp.window(),
            ))
        })
        .collect()
}

pub fn trace_options(link: &Link, start: usize) -> Vec<Vec<u8>> {
    let mut output = Vec::new();
    for entry in link.trace().entries().iter().skip(start) {
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
        let kinds = tcp
            .options()
            .flatten()
            .map(|option| match option {
                TcpOption::WindowScale(_) => 3,
                TcpOption::SackPermitted => 4,
                TcpOption::Sack(_) => 5,
                TcpOption::Timestamps { .. } => 8,
                TcpOption::Md5 { .. } => 19,
                TcpOption::Authentication { .. } => 29,
                TcpOption::MaxSegmentSize(_) => 2,
                TcpOption::EndOfList => 0,
                TcpOption::NoOperation => 1,
                TcpOption::Unknown { kind, .. } => kind,
            })
            .collect();
        output.push(kinds);
    }
    output
}

#[allow(clippy::too_many_arguments)]
pub fn inject_segment(
    stack: &mut Stack<Endpoint>,
    src: [u8; 4],
    dst: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    payload: &[u8],
) {
    inject_segment_with_options(
        stack,
        src,
        dst,
        src_port,
        dst_port,
        seq,
        ack,
        flags,
        payload,
        &[],
    );
}

#[allow(clippy::too_many_arguments)]
pub fn inject_segment_with_options(
    stack: &mut Stack<Endpoint>,
    src: [u8; 4],
    dst: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    payload: &[u8],
    options: &[u8],
) {
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
    let mut segment = vec![0u8; 20 + options.len().div_ceil(4) * 4 + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, options, payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
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
    stack.test_inject_frame(&frame, Instant::ZERO);
}

pub fn trace_tcp(stack: &Stack<Endpoint>) -> Vec<(TcpFlags, u32, u32)> {
    stack
        .test_trace()
        .iter()
        .filter_map(|bytes| {
            let ethernet = EthernetFrame::parse(bytes).ok()?;
            if ethernet.ether_type() != EtherType::Ipv4 {
                return None;
            }
            let ipv4 = Ipv4Packet::parse(ethernet.payload()).ok()?;
            if ipv4.protocol() != 6 {
                return None;
            }
            let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
            let tcp = TcpPacket::parse(ipv4.payload(), context).ok()?;
            Some((tcp.flags(), tcp.sequence(), tcp.acknowledgement()))
        })
        .collect()
}

pub fn null_stack(config: StackConfig, seed: u64) -> Stack<raven_tcpip::testutil::NullDevice> {
    let mut stack = Stack::new(
        raven_tcpip::testutil::NullDevice::new(),
        config,
        seed,
        Instant::ZERO,
    );
    stack
        .set_ipv4(Ipv4Config {
            addr: address(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

#[allow(clippy::too_many_arguments)]
pub fn inject_segment_at(
    stack: &mut Stack<Endpoint>,
    src: [u8; 4],
    dst: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    payload: &[u8],
    options: &[u8],
    now: Instant,
) {
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
    let mut segment = vec![0u8; 20 + options.len().div_ceil(4) * 4 + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, options, payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
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
    stack.test_inject_frame(&frame, now);
}

pub fn inject_fragment(
    stack: &mut Stack<Endpoint>,
    src: [u8; 4],
    dst: [u8; 4],
    id: u16,
    offset: u16,
    more: bool,
    payload: &[u8],
) {
    let ip_header = Ipv4Header {
        id,
        ttl: 64,
        protocol: 17,
        flags: if more {
            raven_tcpip_wire::ipv4::Ipv4Flags::MF
        } else {
            raven_tcpip_wire::ipv4::Ipv4Flags::default()
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
    stack.test_inject_frame(&frame, Instant::ZERO);
}

pub fn inject_icmp_frag_needed(
    stack: &mut Stack<Endpoint>,
    quoted_src: [u8; 4],
    quoted_dst: [u8; 4],
    mtu: u16,
) {
    use raven_tcpip_wire::icmpv4::Icmpv4Builder;
    let mut quoted = vec![0u8; 28];
    let ip_header = Ipv4Header {
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(quoted_src),
        dst: WireIpv4Addr::new(quoted_dst),
        ..Ipv4Header::default()
    };
    let builder = Ipv4Builder::new(&mut quoted, &ip_header, 8).expect("quoted ipv4");
    builder.finish();
    let mut message = vec![0u8; 8 + quoted.len()];
    let mut builder =
        Icmpv4Builder::new(&mut message, 3, 4, u32::from(mtu), quoted.len()).expect("icmp");
    builder.payload_mut().copy_from_slice(&quoted);
    builder.finish();
    let ip_header = Ipv4Header {
        ttl: 64,
        protocol: 1,
        src: WireIpv4Addr::new(quoted_dst),
        dst: WireIpv4Addr::new(quoted_src),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + message.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, message.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&message);
    builder.finish();
    let mut frame = vec![0u8; 14 + packet.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(BMAC),
        MacAddr::new(AMAC),
        EtherType::Ipv4,
        packet.len(),
    )
    .expect("ethernet");
    builder.payload_mut().copy_from_slice(&packet);
    builder.finish();
    stack.test_inject_frame(&frame, Instant::ZERO);
}

#[allow(clippy::too_many_arguments)]
pub fn inject_segment_window(
    stack: &mut Stack<Endpoint>,
    src: [u8; 4],
    dst: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    window: u16,
    payload: &[u8],
) {
    let header = TcpHeader {
        src_port,
        dst_port,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(src), WireIpv4Addr::new(dst));
    let mut segment = vec![0u8; 20 + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, &[], payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
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
    stack.test_inject_frame(&frame, Instant::ZERO);
}
