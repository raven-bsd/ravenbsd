// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration test for the sans-IO response helper against a real wire segment.

use raven_tcpip::IpAddr;
use raven_tcpip::Ipv4Addr;
use raven_tcpip::tcp_respond::{
    ChecksumInput, Endpoint, RespondMode, RespondRequest, TcpHeader as ResponseHeader,
    reset_selection, tcp_respond,
};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ipv4::Ipv4Addr as WireIpv4Addr;
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpPacket};

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

#[test]
fn a_syn_is_refused_with_rst_ack_over_the_wire_tuple() {
    let local = address([192, 0, 2, 10]);
    let remote = address([198, 51, 100, 20]);
    let remote_wire = WireIpv4Addr::new([198, 51, 100, 20]);
    let local_wire = WireIpv4Addr::new([192, 0, 2, 10]);
    let wire_header = TcpHeader {
        src_port: 50_000,
        dst_port: 80,
        sequence: 0x1122_3344,
        acknowledgement: 0,
        flags: TcpFlags::SYN,
        window: 65_535,
        urgent_pointer: 0,
    };
    let mut buffer = [0u8; 64];
    let builder = TcpBuilder::new(
        &mut buffer,
        ChecksumContext::ipv4(remote_wire, local_wire),
        &wire_header,
        &[],
        0,
    )
    .expect("build segment");
    let wire = builder.finish();
    let parsed = TcpPacket::parse(wire, ChecksumContext::ipv4(remote_wire, local_wire))
        .expect("parse segment");

    let incoming = ResponseHeader {
        source_port: parsed.src_port(),
        destination_port: parsed.dst_port(),
        sequence: parsed.sequence(),
        acknowledgement: parsed.acknowledgement(),
        flags: parsed.flags(),
        window: parsed.window(),
        payload_len: u32::try_from(parsed.payload_len()).unwrap_or(0),
    };
    let selection = reset_selection(&incoming);
    assert_eq!(
        selection.flags.bits(),
        TcpFlags::RST.bits() | TcpFlags::ACK.bits()
    );
    assert_eq!(selection.sequence, 0);
    assert_eq!(selection.acknowledgement, 0x1122_3345);

    let request = RespondRequest {
        local: Endpoint {
            address: IpAddr::V4(local),
            port: 80,
        },
        remote: Endpoint {
            address: IpAddr::V4(remote),
            port: 50_000,
        },
        incoming: &incoming,
        mode: RespondMode::Reply,
        connection: None,
    };
    let response = tcp_respond(
        &request,
        selection.acknowledgement,
        selection.sequence,
        selection.flags,
    )
    .expect("response");
    assert_eq!(response.source.port, 80);
    assert_eq!(response.destination.port, 50_000);
    assert_eq!(
        response.checksum,
        ChecksumInput::V4 {
            source: local,
            destination: remote,
            length: 20,
        }
    );
    assert_eq!(
        response.flags.bits(),
        TcpFlags::RST.bits() | TcpFlags::ACK.bits()
    );
    assert_eq!(response.counters.send_acks, 1);
    assert_eq!(response.counters.send_total, 1);
}
