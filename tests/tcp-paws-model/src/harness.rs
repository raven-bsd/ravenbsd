// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic real-stack harness for the timestamps/PAWS model.

use raven_abi::errno::Errno;
use raven_tcpip::{Instant, Ipv4Addr, Ipv4Config, LoopbackDevice, SocketId, Stack, StackConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpPacket};

/// Local address (the `LoopbackDevice` hardware address is `AMAC`).
pub const A: [u8; 4] = [192, 0, 2, 1];
/// Peer address.
pub const B: [u8; 4] = [192, 0, 2, 2];
/// Local MAC.
pub const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
/// Peer MAC.
pub const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];
/// Peer ephemeral port.
pub const PEER_PORT: u16 = 40_000;
/// Local listening port.
pub const LOCAL_PORT: u16 = 80;

/// A host address.
#[must_use]
pub fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

/// A timestamp option (`kind=8, length=10`).
#[must_use]
pub fn timestamp_options(tsval: u32, tsecr: u32) -> Vec<u8> {
    let mut options = vec![8u8, 10];
    options.extend_from_slice(&tsval.to_be_bytes());
    options.extend_from_slice(&tsecr.to_be_bytes());
    options
}

/// A raw IPv4/TCP frame from the peer.
#[allow(clippy::too_many_arguments)] // Mirrors the wire header fields.
pub fn frame(
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    window: u16,
    options: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let header = TcpHeader {
        src_port: PEER_PORT,
        dst_port: LOCAL_PORT,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window,
        urgent_pointer: 0,
    };
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(B), WireIpv4Addr::new(A));
    let padded = (options.len() + 3) & !3;
    let mut segment = vec![0u8; 20 + padded + payload.len()];
    {
        let mut builder =
            TcpBuilder::new(&mut segment, context, &header, options, payload.len()).expect("tcp");
        builder.payload_mut().copy_from_slice(payload);
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 312,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(B),
        dst: WireIpv4Addr::new(A),
        ..Ipv4Header::default()
    };
    let mut packet = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut packet, &ip_header, segment.len()).expect("ipv4");
    builder.payload_mut().copy_from_slice(&segment);
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
    frame
}

/// Parse every TCP segment in a trace.
fn tcp_segments(trace: &[Vec<u8>]) -> Vec<TcpPacket<'_>> {
    let mut segments = Vec::new();
    for entry in trace {
        let Ok(ethernet) = raven_tcpip_wire::ethernet::EthernetFrame::parse(entry) else {
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
        if let Ok(tcp) = TcpPacket::parse(ipv4.payload(), context) {
            segments.push(tcp);
        }
    }
    segments
}

/// The last timestamp option in a trace as `(tsval, tsecr)`.
#[must_use]
pub fn last_timestamp(trace: &[Vec<u8>]) -> Option<(u32, u32)> {
    tcp_segments(trace).into_iter().rev().find_map(|segment| {
        segment.options().find_map(|option| match option {
            Ok(TcpOption::Timestamps { tsval, tsecr }) => Some((tsval, tsecr)),
            _ => None,
        })
    })
}

/// A real stack with one established socket and a negotiated timestamp.
pub struct Established {
    /// The server stack.
    pub stack: Stack<LoopbackDevice>,
    /// The accepted connection.
    pub socket: SocketId,
    /// Timestamp value sent in the SYN-ACK.
    pub synack_ts: u32,
    /// Timestamp echoed by the SYN-ACK (the SYN's TSval).
    pub synack_tsecr: u32,
}

/// Complete a timestamped passive handshake at `irs` and `base_ms`.
#[must_use]
pub fn establish(config: StackConfig, irs: u32, client_ts: u32, base_ms: i64) -> Established {
    let mut stack = Stack::new(
        LoopbackDevice::with_mtu(1514),
        config,
        312,
        Instant::from_millis(base_ms),
    );
    stack
        .set_ipv4(Ipv4Config {
            addr: address(A),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack.test_insert_neighbor(address(B), BMAC, Instant::from_millis(base_ms), 60_000);
    stack.test_set_trace(true);
    let listener = stack.tcp_listen(LOCAL_PORT, 4).expect("listen");
    let syn = frame(
        irs,
        0,
        TcpFlags::SYN,
        65_535,
        &timestamp_options(client_ts, 0),
        &[],
    );
    stack.test_inject_frame(&syn, Instant::from_millis(base_ms));
    stack.poll(Instant::from_millis(base_ms));
    let (iss, syncache_irs, _sack, timestamped) = stack
        .test_syncache_entry(PEER_PORT)
        .expect("syncache entry");
    assert_eq!(syncache_irs, irs, "syncache IRS");
    assert!(timestamped, "syncache timestamp");
    let rcv_nxt = irs.wrapping_add(1);
    let (synack_ts, synack_tsecr) = last_timestamp(stack.test_trace()).expect("SYN-ACK timestamp");
    let ack = frame(
        rcv_nxt,
        iss.wrapping_add(1),
        TcpFlags::ACK,
        65_535,
        &timestamp_options(client_ts.wrapping_add(1), synack_ts),
        &[],
    );
    stack.test_inject_frame(&ack, Instant::from_millis(base_ms + 1));
    stack.poll(Instant::from_millis(base_ms + 1));
    let socket = stack
        .tcp_accept(listener)
        .expect("accept")
        .expect("connection");
    Established {
        stack,
        socket,
        synack_ts,
        synack_tsecr,
    }
}

/// Inject one ACK segment into the established socket.
#[allow(clippy::too_many_arguments)] // Mirrors the wire fields for test readability.
pub fn inject(
    established: &mut Established,
    seq: u32,
    tsval: u32,
    tsecr: u32,
    window: u16,
    payload: &[u8],
    now_ms: i64,
) {
    let ack = established
        .stack
        .test_tcp_window(established.socket)
        .expect("window")
        .2;
    let flags = if payload.is_empty() {
        TcpFlags::ACK
    } else {
        TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::PSH.bits())
    };
    let packet = frame(
        seq,
        ack,
        flags,
        window,
        &timestamp_options(tsval, tsecr),
        payload,
    );
    established
        .stack
        .test_inject_frame(&packet, Instant::from_millis(now_ms));
}

/// Drain the receive queue of an established socket.
#[must_use]
pub fn drain(established: &mut Established) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = vec![0u8; 4096];
    loop {
        match established.stack.recv(established.socket, &mut buffer) {
            Ok(0) => break,
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => break,
            Err(_) => break,
        }
    }
    output
}

/// Current negotiated options: `(timestamps, ts_recent, paws_drops, last_tsval)`.
#[must_use]
pub fn options_state(established: &Established) -> (bool, u32, usize, u32) {
    let info = established
        .stack
        .test_tcp_options_info(established.socket)
        .expect("options");
    (info.2, info.3, info.7, info.8)
}
