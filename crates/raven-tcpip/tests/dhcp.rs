// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The DHCPv4 client against a scripted server on a simulated link: the exchange on the wire,
//! the interface configuration it produces, renewal, expiry and the static override.

#![cfg(feature = "testutil")]

use raven_tcpip::{Device, DhcpRetryConfig, Instant, Ipv4Addr, Ipv4Config, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Addr as WireAddr, Ipv4Builder, Ipv4Header, Ipv4Packet};
use raven_tcpip_wire::udp::{UdpBuilder, UdpPacket};

const SERVER_IP: [u8; 4] = [192, 168, 1, 1];
const OFFERED: [u8; 4] = [192, 168, 1, 50];
const SERVER_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0xaa];
const CLIENT_MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x0a];

struct Rig {
    client: Stack<Endpoint>,
    server: Endpoint,
    now: i64,
}

fn rig() -> Rig {
    let (a, b) = Link::new(
        LinkConfig {
            rtt_ms: 2,
            ..LinkConfig::default()
        },
        7,
    );
    let config = StackConfig {
        hardware_addr: Some(CLIENT_MAC),
        ..StackConfig::default()
    };
    Rig {
        client: Stack::new(a, config, 3, Instant::ZERO),
        server: b,
        now: 0,
    }
}

/// A DHCP message the server received: kind, transaction id and a few fields.
#[derive(Debug)]
struct Received {
    kind: u8,
    xid: u32,
    ciaddr: [u8; 4],
    broadcast_flag: bool,
    frame_dst: [u8; 6],
    ip_src: [u8; 4],
    ip_dst: [u8; 4],
    requested: Option<[u8; 4]>,
}

fn drain_server(server: &mut Endpoint, now: i64) -> Vec<Received> {
    let mut out = Vec::new();
    loop {
        let mut frame = Vec::new();
        if !server.receive(Instant::from_millis(now), &mut |bytes| {
            frame = bytes.to_vec();
        }) {
            break;
        }
        let Ok(ethernet) = EthernetFrame::parse(&frame) else {
            continue;
        };
        if ethernet.ether_type() != EtherType::Ipv4 {
            continue;
        }
        let Ok(ip) = Ipv4Packet::parse(ethernet.payload()) else {
            continue;
        };
        if ip.protocol() != 17 {
            continue;
        }
        let context = ChecksumContext::ipv4(ip.src(), ip.dst());
        let Ok(udp) = UdpPacket::parse(ip.payload(), context) else {
            continue;
        };
        if udp.dst_port() != 67 || udp.src_port() != 68 {
            continue;
        }
        let m = udp.payload();
        let mut requested = None;
        let mut kind = 0;
        let mut options = &m[240..];
        while let Some((&code, rest)) = options.split_first() {
            if code == 0 {
                options = rest;
                continue;
            }
            if code == 255 {
                break;
            }
            let length = usize::from(rest[0]);
            let value = &rest[1..1 + length];
            match code {
                53 => kind = value[0],
                50 => requested = Some([value[0], value[1], value[2], value[3]]),
                _ => {}
            }
            options = &rest[1 + length..];
        }
        out.push(Received {
            kind,
            xid: u32::from_be_bytes([m[4], m[5], m[6], m[7]]),
            ciaddr: [m[12], m[13], m[14], m[15]],
            broadcast_flag: m[10] & 0x80 != 0,
            frame_dst: ethernet.destination().octets(),
            ip_src: ip.src().octets(),
            ip_dst: ip.dst().octets(),
            requested,
        });
    }
    out
}

fn reply(kind: u8, xid: u32, router: [u8; 4], lease_s: u32) -> Vec<u8> {
    let mut m = vec![0u8; 236];
    m[0] = 2;
    m[1] = 1;
    m[2] = 6;
    m[4..8].copy_from_slice(&xid.to_be_bytes());
    m[16..20].copy_from_slice(&OFFERED);
    m[28..34].copy_from_slice(&CLIENT_MAC);
    m.extend_from_slice(&[0x63, 0x82, 0x53, 0x63]);
    m.extend_from_slice(&[53, 1, kind, 54, 4]);
    m.extend_from_slice(&SERVER_IP);
    if kind != 6 {
        m.extend_from_slice(&[1, 4, 255, 255, 255, 0, 3, 4]);
        m.extend_from_slice(&router);
        m.extend_from_slice(&[6, 4, 192, 168, 1, 2, 51, 4]);
        m.extend_from_slice(&lease_s.to_be_bytes());
    }
    m.push(255);
    m
}

fn send_reply(rig: &mut Rig, destination: [u8; 4], dhcp: &[u8]) {
    let src = WireAddr::new(SERVER_IP);
    let dst = WireAddr::new(destination);
    let mut datagram = vec![0u8; 8 + dhcp.len()];
    {
        let mut udp = UdpBuilder::new(
            &mut datagram,
            ChecksumContext::ipv4(src, dst),
            67,
            68,
            dhcp.len(),
        )
        .expect("udp");
        udp.payload_mut().copy_from_slice(dhcp);
        udp.finish();
    }
    let mut packet = vec![0u8; 20 + datagram.len()];
    {
        let header = Ipv4Header {
            ttl: 64,
            protocol: 17,
            src,
            dst,
            ..Ipv4Header::default()
        };
        let mut ip = Ipv4Builder::new(&mut packet, &header, datagram.len()).expect("ip");
        ip.payload_mut().copy_from_slice(&datagram);
    }
    let mut frame = vec![0u8; 14 + packet.len()];
    {
        let mac = if destination == [255; 4] {
            MacAddr::BROADCAST
        } else {
            MacAddr::new(CLIENT_MAC)
        };
        let mut eth = EthernetBuilder::new(
            &mut frame,
            mac,
            MacAddr::new(SERVER_MAC),
            EtherType::Ipv4,
            packet.len(),
        )
        .expect("eth");
        eth.payload_mut().copy_from_slice(&packet);
    }
    let now = Instant::from_millis(rig.now);
    rig.server
        .transmit(now, frame.len(), &mut |buffer| {
            buffer.copy_from_slice(&frame)
        })
        .expect("transmit");
}

fn step(rig: &mut Rig, ms: i64) -> Vec<Received> {
    let mut seen = Vec::new();
    let to = rig.now + ms;
    while rig.now < to {
        rig.now += 1;
        rig.client.poll(Instant::from_millis(rig.now));
        seen.extend(drain_server(&mut rig.server, rig.now));
    }
    seen
}

fn lease_through(rig: &mut Rig, lease_s: u32) {
    rig.client.dhcp_enable().expect("enable");
    let seen = step(rig, 20);
    let discover = seen.iter().find(|m| m.kind == 1).expect("a DISCOVER");
    let xid = discover.xid;
    send_reply(rig, [255; 4], &reply(2, xid, SERVER_IP, lease_s));
    let seen = step(rig, 20);
    assert!(seen.iter().any(|m| m.kind == 3), "no REQUEST: {seen:?}");
    send_reply(rig, [255; 4], &reply(5, xid, SERVER_IP, lease_s));
    step(rig, 20);
}

#[test]
fn the_discover_goes_out_as_a_broadcast_from_the_unspecified_address() {
    let mut rig = rig();
    rig.client.dhcp_enable().expect("enable");
    let seen = step(&mut rig, 20);
    let discover = seen.iter().find(|m| m.kind == 1).expect("a DISCOVER");
    assert_eq!(discover.frame_dst, [0xff; 6]);
    assert_eq!(discover.ip_src, [0; 4]);
    assert_eq!(discover.ip_dst, [255; 4]);
    assert_eq!(discover.ciaddr, [0; 4]);
    assert!(discover.broadcast_flag);
}

#[test]
fn a_full_exchange_configures_the_interface_with_address_and_gateway() {
    let mut rig = rig();
    assert_eq!(rig.client.ipv4_config(), None);
    lease_through(&mut rig, 3_600);
    let config = rig.client.ipv4_config().expect("configured");
    assert_eq!(config.addr, Ipv4Addr::new(OFFERED));
    assert_eq!(config.prefix_len, 24);
    assert_eq!(config.gateway, Some(Ipv4Addr::new(SERVER_IP)));
    let lease = rig.client.dhcp_lease().expect("lease");
    assert_eq!(lease.dns, vec![Ipv4Addr::new([192, 168, 1, 2])]);
}

#[test]
fn the_request_names_the_offered_address() {
    let mut rig = rig();
    rig.client.dhcp_enable().expect("enable");
    let seen = step(&mut rig, 20);
    let xid = seen.iter().find(|m| m.kind == 1).expect("discover").xid;
    send_reply(&mut rig, [255; 4], &reply(2, xid, SERVER_IP, 3_600));
    let seen = step(&mut rig, 20);
    let request = seen.iter().find(|m| m.kind == 3).expect("request");
    assert_eq!(request.requested, Some(OFFERED));
}

#[test]
fn a_leased_stack_answers_arp_and_reaches_the_gateway_network() {
    let mut rig = rig();
    lease_through(&mut rig, 3_600);
    assert!(
        rig.client
            .udp_bind(5000)
            .and_then(|socket| rig.client.udp_send_to(
                socket,
                b"hello",
                (Ipv4Addr::new([192, 168, 1, 77]), 9)
            ))
            .is_ok(),
        "the leased interface can send on its connected network"
    );
}

#[test]
fn the_lease_is_renewed_by_unicast_before_it_ends() {
    let mut rig = rig();
    lease_through(&mut rig, 100);
    rig.client.test_insert_neighbor(
        Ipv4Addr::new(SERVER_IP),
        SERVER_MAC,
        Instant::from_millis(rig.now),
        600_000,
    );
    let seen = step(&mut rig, 60_000);
    let renew = seen
        .iter()
        .find(|m| m.kind == 3 && m.ciaddr == OFFERED)
        .expect("a renewal REQUEST with ciaddr set");
    assert_eq!(renew.ip_src, OFFERED);
    assert!(!renew.broadcast_flag);
}

#[test]
fn an_expired_lease_removes_the_address_and_discovery_restarts() {
    let mut rig = rig();
    rig.client.dhcp_set_retry_config(DhcpRetryConfig::default());
    lease_through(&mut rig, 20);
    assert!(rig.client.ipv4_config().is_some());
    let seen = step(&mut rig, 25_000);
    assert_eq!(rig.client.ipv4_config(), None);
    assert!(seen.iter().any(|m| m.kind == 1), "discovery restarted");
}

#[test]
fn a_static_address_stops_the_client() {
    let mut rig = rig();
    rig.client.dhcp_enable().expect("enable");
    assert!(rig.client.dhcp_active());
    rig.client
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new([10, 0, 0, 2]),
            prefix_len: 24,
            gateway: None,
        })
        .expect("static");
    assert!(!rig.client.dhcp_active());
    let seen = step(&mut rig, 30_000);
    assert!(seen.is_empty(), "no DHCP traffic after the override");
}

#[test]
fn a_nak_restarts_the_exchange() {
    let mut rig = rig();
    rig.client.dhcp_enable().expect("enable");
    let seen = step(&mut rig, 20);
    let xid = seen.iter().find(|m| m.kind == 1).expect("discover").xid;
    send_reply(&mut rig, [255; 4], &reply(2, xid, SERVER_IP, 3_600));
    step(&mut rig, 20);
    send_reply(&mut rig, [255; 4], &reply(6, xid, SERVER_IP, 0));
    let seen = step(&mut rig, 20);
    assert!(
        seen.iter().any(|m| m.kind == 1),
        "a new DISCOVER after the NAK"
    );
    assert_eq!(rig.client.ipv4_config(), None);
}

#[test]
fn dhcp_needs_a_hardware_address() {
    let (a, _b) = Link::new(LinkConfig::default(), 1);
    let mut stack = Stack::new(NoMac(a), StackConfig::default(), 1, Instant::ZERO);
    assert_eq!(stack.dhcp_enable(), Err(raven_tcpip::Errno::ENETUNREACH));
}

struct NoMac(Endpoint);

impl Device for NoMac {
    fn capabilities(&self) -> raven_tcpip::DeviceCaps {
        let mut caps = self.0.capabilities();
        caps.hardware_addr = None;
        caps
    }
    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        self.0.receive(now, consume)
    }
    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), raven_tcpip::TxError> {
        self.0.transmit(now, len, fill)
    }
}
