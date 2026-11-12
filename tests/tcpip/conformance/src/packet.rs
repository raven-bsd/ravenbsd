// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Frame construction and parsing for the conformance runner.

use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpPacket};

use crate::script::{OptionSpec, PacketSpec};

/// Stack-under-test IPv4 address.
pub const SUT_ADDR: [u8; 4] = [10, 0, 0, 1];
/// Scripted-peer IPv4 address.
pub const PEER_ADDR: [u8; 4] = [10, 0, 0, 2];
/// Stack-under-test MAC address (endpoint A of the simulator).
pub const SUT_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0a];
/// Scripted-peer MAC address.
pub const PEER_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0b];
/// Stack-under-test TCP port.
pub const PORT: u16 = 8080;
/// Scripted-peer TCP port.
pub const PEER_PORT: u16 = 12345;
/// Scripted-peer initial sequence number.
pub const PEER_ISN: u32 = 1000;

/// A parsed TCP packet from the SUT trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    /// Raw flags.
    pub flags: u8,
    /// Absolute sequence number.
    pub seq: u32,
    /// Absolute acknowledgement number.
    pub ack: u32,
    /// Advertised window.
    pub win: u16,
    /// Payload length.
    pub len: u32,
    /// TCP options as kind numbers with optional bodies.
    pub options: Vec<TcpOption<'static>>,
}

impl Parsed {
    /// Whether the SYN flag is set.
    #[must_use]
    pub fn syn(&self) -> bool {
        self.flags & 0x02 != 0
    }
}

/// Build an Ethernet frame from the scripted peer to the SUT.
#[must_use]
pub fn build_inbound(packet: &PacketSpec, sut_isn: u32, src_port: u16, dst_port: u16) -> Vec<u8> {
    let seq = PEER_ISN.wrapping_add(packet.seq);
    let ack = if let Some(ack) = packet.ack {
        sut_isn.wrapping_add(ack)
    } else {
        0
    };
    let flags = TcpFlags::from_bits(packet.flags);
    let header = TcpHeader {
        src_port,
        dst_port,
        sequence: seq,
        acknowledgement: ack,
        flags,
        window: packet.win.unwrap_or(65_535),
        urgent_pointer: 0,
    };
    let options = encode_options(&packet.options);
    let payload = vec![b'x'; usize::try_from(packet.len).unwrap_or(0)];
    let context = ChecksumContext::ipv4(WireIpv4Addr::new(PEER_ADDR), WireIpv4Addr::new(SUT_ADDR));
    let mut segment = vec![0u8; 20 + options.len().div_ceil(4) * 4 + payload.len()];
    {
        let mut builder = TcpBuilder::new(&mut segment, context, &header, &options, payload.len())
            .expect("tcp builder");
        builder.payload_mut().copy_from_slice(&payload);
        builder.finish();
    }
    let ip_header = Ipv4Header {
        id: 17,
        ttl: 64,
        protocol: 6,
        src: WireIpv4Addr::new(PEER_ADDR),
        dst: WireIpv4Addr::new(SUT_ADDR),
        ..Ipv4Header::default()
    };
    let mut ip = vec![0u8; 20 + segment.len()];
    let mut builder = Ipv4Builder::new(&mut ip, &ip_header, segment.len()).expect("ipv4 builder");
    builder.payload_mut().copy_from_slice(&segment);
    builder.finish();
    let mut frame = vec![0u8; 14 + ip.len()];
    let mut builder = EthernetBuilder::new(
        &mut frame,
        MacAddr::new(PEER_MAC),
        MacAddr::new(SUT_MAC),
        EtherType::Ipv4,
        ip.len(),
    )
    .expect("ethernet builder");
    builder.payload_mut().copy_from_slice(&ip);
    builder.finish();
    frame
}

fn encode_options(options: &[OptionSpec]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for option in options {
        match option {
            OptionSpec::Mss(value) => {
                bytes.extend_from_slice(&[2, 4]);
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            OptionSpec::Wscale(value) => bytes.extend_from_slice(&[3, 3, *value]),
            OptionSpec::SackOk => bytes.extend_from_slice(&[4, 2]),
            OptionSpec::TimestampsAny => {}
            OptionSpec::Timestamps { val, ecr } => {
                bytes.extend_from_slice(&[8, 10]);
                bytes.extend_from_slice(&val.to_be_bytes());
                bytes.extend_from_slice(&ecr.to_be_bytes());
            }
            OptionSpec::Nop => bytes.push(1),
            OptionSpec::Eol => bytes.push(0),
            OptionSpec::Unknown(kind) => bytes.extend_from_slice(&[*kind, 2]),
        }
    }
    bytes
}

/// Parse a TCP/IPv4 frame from the link trace.
#[must_use]
pub fn parse_trace_frame(frame: &[u8]) -> Option<Parsed> {
    let ethernet = EthernetFrame::parse(frame).ok()?;
    if ethernet.ether_type() != EtherType::Ipv4 {
        return None;
    }
    let ipv4 = Ipv4Packet::parse(ethernet.payload()).ok()?;
    if ipv4.protocol() != 6 {
        return None;
    }
    let context = ChecksumContext::ipv4(ipv4.src(), ipv4.dst());
    let tcp = TcpPacket::parse(ipv4.payload(), context).ok()?;
    let options = tcp
        .options()
        .flatten()
        .map(|option| match option {
            TcpOption::EndOfList => TcpOption::EndOfList,
            TcpOption::NoOperation => TcpOption::NoOperation,
            TcpOption::MaxSegmentSize(value) => TcpOption::MaxSegmentSize(value),
            TcpOption::WindowScale(value) => TcpOption::WindowScale(value),
            TcpOption::SackPermitted => TcpOption::SackPermitted,
            TcpOption::Timestamps { tsval, tsecr } => TcpOption::Timestamps { tsval, tsecr },
            TcpOption::Md5 { .. } => TcpOption::Unknown {
                kind: 19,
                data: &[],
            },
            TcpOption::Authentication { .. } => TcpOption::Unknown {
                kind: 29,
                data: &[],
            },
            TcpOption::Sack(_) => TcpOption::Unknown { kind: 5, data: &[] },
            TcpOption::Unknown { kind, .. } => TcpOption::Unknown { kind, data: &[] },
        })
        .collect();
    Some(Parsed {
        flags: tcp.flags().bits(),
        seq: tcp.sequence(),
        ack: tcp.acknowledgement(),
        win: tcp.window(),
        len: u32::try_from(tcp.payload().len()).unwrap_or(0),
        options,
    })
}

/// Whether `parsed` contains every option in `expected`.
#[must_use]
pub fn options_match(parsed: &[TcpOption<'static>], expected: &[OptionSpec]) -> bool {
    expected.iter().all(|option| {
        match option {
        OptionSpec::Mss(value) => parsed
            .iter()
            .any(|actual| matches!(actual, TcpOption::MaxSegmentSize(v) if v == value)),
        OptionSpec::Wscale(value) => parsed
            .iter()
            .any(|actual| matches!(actual, TcpOption::WindowScale(v) if v == value)),
        OptionSpec::SackOk => parsed
            .iter()
            .any(|actual| matches!(actual, TcpOption::SackPermitted)),
        OptionSpec::TimestampsAny => parsed
            .iter()
            .any(|actual| matches!(actual, TcpOption::Timestamps { .. })),
        OptionSpec::Timestamps { val, ecr } => parsed.iter().any(|actual| {
            matches!(actual, TcpOption::Timestamps { tsval, tsecr } if tsval == val && tsecr == ecr)
        }),
        OptionSpec::Nop => parsed.iter().any(|actual| matches!(actual, TcpOption::NoOperation)),
        OptionSpec::Eol => parsed.iter().any(|actual| matches!(actual, TcpOption::EndOfList)),
        OptionSpec::Unknown(kind) => parsed
            .iter()
            .any(|actual| matches!(actual, TcpOption::Unknown { kind: k, .. } if k == kind)),
    }
    })
}
