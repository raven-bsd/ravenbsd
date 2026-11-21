// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the device-boundary filter hook.

use super::*;
use alloc::collections::VecDeque;
use alloc::vec;

use raven_pf::{Ipv4Addr, Ipv6Addr};

const CLIENT: [u8; 4] = [10, 0, 0, 5];
const SERVER: [u8; 4] = [192, 0, 2, 10];
const NATIP: [u8; 4] = [203, 0, 113, 7];

struct Wire {
    caps: DeviceCaps,
    rx: VecDeque<Vec<u8>>,
    tx: Vec<Vec<u8>>,
}

impl Wire {
    fn ethernet() -> Self {
        Self {
            caps: DeviceCaps {
                medium: Medium::Ethernet,
                mtu: 1514,
                hardware_addr: Some([0x02, 0, 0, 0, 0, 1]),
                checksum_offload: raven_netcore::ChecksumOffload::None,
            },
            rx: VecDeque::new(),
            tx: Vec::new(),
        }
    }

    fn queue(&mut self, frame: Vec<u8>) {
        self.rx.push_back(frame);
    }
}

impl Device for Wire {
    fn capabilities(&self) -> DeviceCaps {
        self.caps
    }

    fn receive(&mut self, _now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let Some(frame) = self.rx.pop_front() else {
            return false;
        };
        consume(&frame);
        true
    }

    fn transmit(
        &mut self,
        _now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        let mut frame = vec![0u8; len];
        fill(&mut frame);
        self.tx.push(frame);
        Ok(())
    }
}

fn udp_frame(
    source: [u8; 4],
    source_port: u16,
    destination: [u8; 4],
    destination_port: u16,
) -> Vec<u8> {
    let payload = b"pf";
    let mut frame = vec![0u8; 14 + 20 + 8 + payload.len()];
    frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    frame[14] = 0x45;
    let total = 20 + 8 + payload.len();
    frame[16..18].copy_from_slice(&u16::try_from(total).unwrap().to_be_bytes());
    frame[22] = 64;
    frame[23] = 17;
    frame[26..30].copy_from_slice(&source);
    frame[30..34].copy_from_slice(&destination);
    let header = frame[14..34].to_vec();
    let sum = checksum(&header);
    frame[24..26].copy_from_slice(&sum.to_be_bytes());
    frame[34..36].copy_from_slice(&source_port.to_be_bytes());
    frame[36..38].copy_from_slice(&destination_port.to_be_bytes());
    let length = u16::try_from(8 + payload.len()).unwrap();
    frame[38..40].copy_from_slice(&length.to_be_bytes());
    frame[42..].copy_from_slice(payload);
    let mut sum = add_bytes(0, &source);
    sum = add_bytes(sum, &destination);
    sum = add_bytes(sum, &[0, 17, (length >> 8) as u8, (length & 0xff) as u8]);
    sum = add_bytes(sum, &frame[34..]);
    let sum = fold(sum);
    let sum = if sum == 0 { 0xffff } else { sum };
    frame[40..42].copy_from_slice(&sum.to_be_bytes());
    frame
}

fn udp6_frame(
    source: Ipv6Addr,
    source_port: u16,
    destination: Ipv6Addr,
    destination_port: u16,
) -> Vec<u8> {
    let payload = b"pf6";
    let mut frame = vec![0u8; 14 + 40 + 8 + payload.len()];
    frame[12..14].copy_from_slice(&0x86ddu16.to_be_bytes());
    frame[14] = 0x60;
    let length = u16::try_from(8 + payload.len()).unwrap();
    frame[18..20].copy_from_slice(&length.to_be_bytes());
    frame[20] = 17;
    frame[21] = 64;
    frame[22..38].copy_from_slice(&source.octets());
    frame[38..54].copy_from_slice(&destination.octets());
    frame[54..56].copy_from_slice(&source_port.to_be_bytes());
    frame[56..58].copy_from_slice(&destination_port.to_be_bytes());
    frame[58..60].copy_from_slice(&length.to_be_bytes());
    frame[62..].copy_from_slice(payload);
    let mut sum = add_bytes(0, &source.octets());
    sum = add_bytes(sum, &destination.octets());
    sum = add_bytes(sum, &(u32::from(length)).to_be_bytes());
    sum = add_bytes(sum, &[0, 0, 0, 17]);
    sum = add_bytes(sum, &frame[54..]);
    let sum = fold(sum);
    let sum = if sum == 0 { 0xffff } else { sum };
    frame[60..62].copy_from_slice(&sum.to_be_bytes());
    frame
}

fn arp_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 42];
    frame[12..14].copy_from_slice(&0x0806u16.to_be_bytes());
    frame
}

fn valid_ipv4_header(frame: &[u8]) -> bool {
    checksum(&frame[14..34]) == 0
}

fn valid_udp_checksum(frame: &[u8], v6: bool) -> bool {
    if v6 {
        let mut sum = add_bytes(0, &frame[22..38]);
        sum = add_bytes(sum, &frame[38..54]);
        sum = add_bytes(
            sum,
            &u32::from(u16::from_be_bytes([frame[18], frame[19]])).to_be_bytes(),
        );
        sum = add_bytes(sum, &[0, 0, 0, 17]);
        sum = add_bytes(sum, &frame[54..]);
        fold(sum) == 0
    } else {
        let mut sum = add_bytes(0, &frame[26..30]);
        sum = add_bytes(sum, &frame[30..34]);
        sum = add_bytes(sum, &[0, 17, frame[38], frame[39]]);
        sum = add_bytes(sum, &frame[34..]);
        fold(sum) == 0
    }
}

#[test]
fn a_block_rule_drops_an_ingress_frame_and_logs_it() {
    let policy =
        FilterPolicy::from_text("block in on lo proto udp from any to any port 9999 log\n")
            .unwrap();
    let mut wire = Wire::ethernet();
    wire.queue(udp_frame(CLIENT, 40000, SERVER, 9999));
    let mut device = FilterDevice::new(wire, Some(policy.clone()), "lo");
    let mut forwarded = false;
    device.receive(Instant::from_millis(0), &mut |_| forwarded = true);
    assert!(!forwarded);
    assert_eq!(device.dropped(), 1);
    let lines = policy.drain_log();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("pf block in lo udp"), "{:?}", lines);
    assert!(lines[0].contains("192.0.2.10:9999"), "{:?}", lines);
}

#[test]
fn a_pass_rule_forwards_and_the_reply_hits_the_state() {
    let policy = FilterPolicy::from_text(
        "set default block\n\
         pass out on lo proto udp from any to any port 53 keep state\n",
    )
    .unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy), "lo");
    let frame = udp_frame(CLIENT, 40000, SERVER, 53);
    device
        .transmit(Instant::from_millis(0), frame.len(), &mut |out| {
            out.copy_from_slice(&frame)
        })
        .unwrap();
    assert_eq!(device.inner.tx.len(), 1);
    // The reply is admitted by the state created by the outbound frame.
    device.inner.queue(udp_frame(SERVER, 53, CLIENT, 40000));
    let mut forwarded = false;
    device.receive(Instant::from_millis(1_000), &mut |_| forwarded = true);
    assert!(forwarded);
}

#[test]
fn nat_rewrites_the_source_and_both_checksums() {
    let policy = FilterPolicy::from_text(
        "set default pass\n\
         pass out on lo proto udp from any to any port 53 keep state\n\
         nat on lo from any to any -> 203.0.113.7\n",
    )
    .unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy), "lo");
    let frame = udp_frame(CLIENT, 40000, SERVER, 53);
    device
        .transmit(Instant::from_millis(0), frame.len(), &mut |out| {
            out.copy_from_slice(&frame)
        })
        .unwrap();
    let sent = &device.inner.tx[0];
    assert_eq!(&sent[26..30], &NATIP);
    assert_eq!(
        &sent[34..36],
        &raven_pf::nat::NAT_PROXY_PORT_LOW.to_be_bytes()
    );
    assert!(valid_ipv4_header(sent));
    assert!(valid_udp_checksum(sent, false));
}

#[test]
fn a_nat_reply_is_rewritten_back_to_the_internal_tuple() {
    let policy = FilterPolicy::from_text(
        "set default pass\n\
         pass out on lo proto udp from any to any port 53 keep state\n\
         nat on lo from any to any -> 203.0.113.7\n",
    )
    .unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy), "lo");
    let outbound = udp_frame(CLIENT, 40000, SERVER, 53);
    device
        .transmit(Instant::from_millis(0), outbound.len(), &mut |out| {
            out.copy_from_slice(&outbound)
        })
        .unwrap();
    let translated_port = u16::from_be_bytes([device.inner.tx[0][34], device.inner.tx[0][35]]);
    let reply = udp_frame(SERVER, 53, NATIP, translated_port);
    device.inner.queue(reply);
    let mut forwarded = None;
    device.receive(Instant::from_millis(1_000), &mut |bytes| {
        forwarded = Some(bytes.to_vec());
    });
    let forwarded = forwarded.expect("stateful reply");
    assert_eq!(&forwarded[26..30], &SERVER);
    assert_eq!(&forwarded[30..34], &CLIENT);
    assert_eq!(&forwarded[36..38], &40000u16.to_be_bytes());
    assert!(valid_ipv4_header(&forwarded));
    assert!(valid_udp_checksum(&forwarded, false));
}

#[test]
fn redirect_rewrites_the_destination() {
    let policy = FilterPolicy::from_text(
        "set default pass\n\
         pass in on lo proto udp from any to any port 8080 keep state\n\
         rdr on lo proto udp from any to 203.0.113.7 port 53 -> 10.0.0.80 port 8080\n",
    )
    .unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy), "lo");
    device.inner.queue(udp_frame(CLIENT, 40000, NATIP, 53));
    let mut forwarded = None;
    device.receive(Instant::from_millis(0), &mut |bytes| {
        forwarded = Some(bytes.to_vec());
    });
    let forwarded = forwarded.expect("redirected frame");
    assert_eq!(&forwarded[30..34], &[10, 0, 0, 80]);
    assert_eq!(&forwarded[36..38], &8080u16.to_be_bytes());
    assert!(valid_ipv4_header(&forwarded));
    assert!(valid_udp_checksum(&forwarded, false));
}

#[test]
fn ipv6_frames_are_filtered_and_rewritten() {
    let policy = FilterPolicy::from_text(
        "set default pass\n\
         block in inet6 proto udp from any to any port 9999 log\n\
         pass out inet6 proto udp from any to any port 53 keep state\n\
         nat on lo from any to any -> 2001:db8::7\n",
    )
    .unwrap();
    let source = Ipv6Addr::parse("2001:db8::5").unwrap();
    let destination = Ipv6Addr::parse("2001:db8::10").unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy.clone()), "lo");
    device
        .inner
        .queue(udp6_frame(source, 40000, destination, 9999));
    let mut forwarded = false;
    device.receive(Instant::from_millis(0), &mut |_| forwarded = true);
    assert!(!forwarded);
    assert_eq!(device.dropped(), 1);

    let frame = udp6_frame(source, 40000, destination, 53);
    device
        .transmit(Instant::from_millis(1_000), frame.len(), &mut |out| {
            out.copy_from_slice(&frame)
        })
        .unwrap();
    let sent = device.inner.tx.last().unwrap();
    assert_eq!(
        &sent[22..38],
        &Ipv6Addr::parse("2001:db8::7").unwrap().octets()
    );
    assert_eq!(
        &sent[54..56],
        &raven_pf::nat::NAT_PROXY_PORT_LOW.to_be_bytes()
    );
    assert!(valid_udp_checksum(sent, true));
}

#[test]
fn non_ip_frames_pass_unfiltered() {
    let policy = FilterPolicy::from_text("block in all log\n").unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy), "lo");
    device.inner.queue(arp_frame());
    let mut forwarded = false;
    device.receive(Instant::from_millis(0), &mut |_| forwarded = true);
    assert!(forwarded);
    assert_eq!(device.dropped(), 0);
}

#[test]
fn the_config_loader_rejects_a_bad_rule() {
    let error = FilterPolicy::from_text("pass in proto quic from any to any\n").unwrap_err();
    assert!(error.contains("unknown protocol"), "{error}");
}

#[test]
fn a_full_state_table_fails_closed() {
    let ruleset = parse("pass in on lo proto udp from any to any keep state\n").unwrap();
    let policy = FilterPolicy::new(ruleset);
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy.clone()), "lo");
    // Fill the table through the policy's own engine by sending many distinct flows.
    for index in 0..=raven_pf::MAX_STATES {
        let source_port = u16::try_from(1 + (index % 60_000)).unwrap();
        device
            .inner
            .queue(udp_frame(CLIENT, source_port, SERVER, 53));
        device.receive(Instant::from_millis(0), &mut |_| {});
    }
    assert_eq!(device.dropped(), 1);
    let lines = policy.drain_log();
    assert!(
        lines.iter().any(|line| line.contains("state-limit")),
        "{:?}",
        lines
    );
}

#[test]
fn a_translation_without_a_usable_pool_fails_closed() {
    let policy = FilterPolicy::from_text(
        "set default pass\n\
         pass out on lo proto udp from any to any port 53 keep state\n\
         nat on lo from any to any -> 2001:db8::7\n",
    )
    .unwrap();
    let mut device = FilterDevice::new(Wire::ethernet(), Some(policy.clone()), "lo");
    let frame = udp_frame(CLIENT, 40000, SERVER, 53);
    device
        .transmit(Instant::from_millis(0), frame.len(), &mut |out| {
            out.copy_from_slice(&frame)
        })
        .unwrap();
    assert!(device.inner.tx.is_empty());
    assert_eq!(device.dropped(), 1);
    assert!(
        policy
            .drain_log()
            .iter()
            .any(|line| line.contains("map-failed"))
    );
}

#[test]
fn the_internal_address_helpers_are_consistent() {
    assert_eq!(
        IpAddr::parse("203.0.113.7").unwrap(),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))
    );
}
