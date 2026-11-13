// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use alloc::vec;

use super::*;

const MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x07];
const SERVER: Ipv4Addr = Ipv4Addr::new([192, 168, 1, 1]);
const OFFERED: Ipv4Addr = Ipv4Addr::new([192, 168, 1, 50]);

fn reply(kind: MessageType, xid: u32, extra: &[(u8, &[u8])]) -> Vec<u8> {
    let mut message = vec![0u8; FIXED_LEN];
    message[0] = 2;
    message[1] = 1;
    message[2] = 6;
    message[4..8].copy_from_slice(&xid.to_be_bytes());
    message[16..20].copy_from_slice(&OFFERED.octets());
    message[28..34].copy_from_slice(&MAC);
    message.extend_from_slice(&MAGIC);
    message.extend_from_slice(&[OPT_MESSAGE_TYPE, 1, kind.code()]);
    message.extend_from_slice(&[OPT_SERVER_ID, 4]);
    message.extend_from_slice(&SERVER.octets());
    for (code, value) in extra {
        message.push(*code);
        message.push(value.len() as u8);
        message.extend_from_slice(value);
    }
    message.push(OPT_END);
    message
}

fn ack_options(lease_s: u32) -> Vec<(u8, Vec<u8>)> {
    vec![
        (OPT_SUBNET_MASK, vec![255, 255, 255, 0]),
        (OPT_ROUTER, vec![192, 168, 1, 1]),
        (OPT_DNS, vec![192, 168, 1, 2, 0, 0, 0, 0, 192, 168, 1, 3]),
        (OPT_LEASE, lease_s.to_be_bytes().to_vec()),
    ]
}

fn borrowed(options: &[(u8, Vec<u8>)]) -> Vec<(u8, &[u8])> {
    options.iter().map(|(c, v)| (*c, v.as_slice())).collect()
}

fn offered(client: &mut Dhcp, now: i64) -> Outgoing {
    let discover = client.poll(now).expect("discover");
    assert_eq!(discover.kind, MessageType::Discover);
    client.on_packet(now, SERVER, &reply(MessageType::Offer, discover.xid, &[]));
    let request = client.poll(now).expect("request");
    assert_eq!(request.kind, MessageType::Request);
    request
}

fn bound(client: &mut Dhcp, now: i64, lease_s: u32) -> Lease {
    let request = offered(client, now);
    assert_eq!(request.requested, Some(OFFERED));
    assert_eq!(request.server_id, Some(SERVER));
    let options = ack_options(lease_s);
    client.on_packet(
        now,
        SERVER,
        &reply(MessageType::Ack, request.xid, &borrowed(&options)),
    );
    match client.take_event() {
        Some(Event::Configured(lease)) => lease,
        other => panic!("expected a lease, got {other:?}"),
    }
}

#[test]
fn the_first_poll_discovers_and_a_second_waits_for_the_timeout() {
    let mut client = Dhcp::new(MAC, 5);
    let first = client.poll(0).expect("discover");
    assert_eq!(first.kind, MessageType::Discover);
    assert_eq!(first.unicast_to, None);
    assert_eq!(client.poll(9_999), None);
    assert!(client.poll(10_000).is_some());
}

#[test]
fn a_full_exchange_yields_a_lease_with_router_and_dns() {
    let mut client = Dhcp::new(MAC, 5);
    let lease = bound(&mut client, 1_000, 3_600);
    assert_eq!(lease.address, OFFERED);
    assert_eq!(lease.prefix_len, 24);
    assert_eq!(lease.router, Some(SERVER));
    assert_eq!(
        lease.dns,
        vec![
            Ipv4Addr::new([192, 168, 1, 2]),
            Ipv4Addr::new([192, 168, 1, 3])
        ],
        "the unspecified DNS entry is dropped"
    );
    assert_eq!(client.lease(), Some(&lease));
}

#[test]
fn a_bound_lease_exposes_its_absolute_timers() {
    let mut client = Dhcp::new(MAC, 5);
    let _ = bound(&mut client, 1_000, 3_600);
    assert_eq!(
        client.timers(),
        Some(DhcpTimers {
            renew_at: 1_801_000,
            rebind_at: 3_151_000,
            expires_at: 3_601_000,
        })
    );
    assert_eq!(Dhcp::new(MAC, 5).timers(), None);
}

#[test]
fn replies_for_another_transaction_or_client_are_ignored() {
    let mut client = Dhcp::new(MAC, 5);
    let discover = client.poll(0).expect("discover");
    client.on_packet(
        0,
        SERVER,
        &reply(MessageType::Offer, discover.xid.wrapping_add(1), &[]),
    );
    assert!(
        client.poll(0).is_none(),
        "still waiting for a matching offer"
    );
    let mut stranger = reply(MessageType::Offer, discover.xid, &[]);
    stranger[28] = 0x0a;
    client.on_packet(0, SERVER, &stranger);
    assert!(client.poll(0).is_none());
}

#[test]
fn an_offer_without_a_server_identifier_is_ignored() {
    let mut client = Dhcp::new(MAC, 5);
    let discover = client.poll(0).expect("discover");
    let mut offer = reply(MessageType::Offer, discover.xid, &[]);
    let at = offer
        .windows(2)
        .position(|pair| pair == [OPT_SERVER_ID, 4])
        .expect("option");
    offer[at] = OPT_PAD;
    offer[at + 1] = OPT_PAD;
    offer[at + 2..at + 6].fill(OPT_PAD);
    client.on_packet(0, SERVER, &offer);
    assert!(client.poll(0).is_none());
}

#[test]
fn request_retries_back_off_then_discovery_restarts() {
    let mut client = Dhcp::new(MAC, 5);
    let discover = client.poll(0).expect("discover");
    client.on_packet(0, SERVER, &reply(MessageType::Offer, discover.xid, &[]));
    let mut times = Vec::new();
    let mut now = 0;
    while let Some(out) = client.poll(now) {
        assert_eq!(out.kind, MessageType::Request);
        times.push(now);
        now = client.poll_at();
    }
    assert_eq!(times, vec![0, 5_000, 10_000, 20_000, 30_000]);
    assert_eq!(
        client.poll(now).map(|out| out.kind),
        Some(MessageType::Discover)
    );
}

#[test]
fn a_nak_while_requesting_restarts_discovery() {
    let mut client = Dhcp::new(MAC, 5);
    let request = offered(&mut client, 0);
    client.on_packet(0, SERVER, &reply(MessageType::Nak, request.xid, &[]));
    assert_eq!(
        client.poll(0).map(|out| out.kind),
        Some(MessageType::Discover)
    );
}

#[test]
fn the_lease_is_renewed_by_unicast_at_t1() {
    let mut client = Dhcp::new(MAC, 5);
    bound(&mut client, 0, 1_000);
    assert_eq!(client.poll(499_999), None);
    let renew = client.poll(500_000).expect("renew");
    assert_eq!(renew.kind, MessageType::Request);
    assert_eq!(renew.ciaddr, OFFERED);
    assert_eq!(renew.unicast_to, Some(SERVER));
    assert_eq!(renew.requested, None);
}

#[test]
fn an_unanswered_renewal_becomes_a_broadcast_rebind_after_t2() {
    let mut client = Dhcp::new(MAC, 5);
    bound(&mut client, 0, 1_000);
    let mut now = 500_000;
    let mut sent = Vec::new();
    while now < 1_000_000 {
        if let Some(out) = client.poll(now) {
            sent.push((now, out.unicast_to.is_none()));
        }
        now = client.poll_at().max(now + 1);
    }
    let (first_broadcast, _) = sent
        .iter()
        .copied()
        .find(|(_, broadcast)| *broadcast)
        .expect("a rebind broadcast");
    assert!(
        first_broadcast >= 875_000,
        "rebind before T2 at {first_broadcast}"
    );
    assert!(
        sent.iter()
            .take_while(|(_, b)| !*b)
            .all(|(at, _)| *at < 875_000),
        "unicast renewals stop at T2"
    );
}

#[test]
fn an_ack_while_renewing_extends_the_lease_without_an_event() {
    let mut client = Dhcp::new(MAC, 5);
    bound(&mut client, 0, 1_000);
    let renew = client.poll(500_000).expect("renew");
    let options = ack_options(1_000);
    client.on_packet(
        500_100,
        SERVER,
        &reply(MessageType::Ack, renew.xid, &borrowed(&options)),
    );
    assert_eq!(client.take_event(), None, "unchanged parameters are silent");
    assert_eq!(
        client.poll(900_000),
        None,
        "T1 moved forward from the new ack"
    );
}

#[test]
fn a_changed_ack_while_renewing_reports_the_new_lease() {
    let mut client = Dhcp::new(MAC, 5);
    bound(&mut client, 0, 1_000);
    let renew = client.poll(500_000).expect("renew");
    let mut options = ack_options(1_000);
    options[1].1 = vec![192, 168, 1, 254];
    client.on_packet(
        500_100,
        SERVER,
        &reply(MessageType::Ack, renew.xid, &borrowed(&options)),
    );
    match client.take_event() {
        Some(Event::Configured(lease)) => {
            assert_eq!(lease.router, Some(Ipv4Addr::new([192, 168, 1, 254])));
        }
        other => panic!("expected a changed lease, got {other:?}"),
    }
}

#[test]
fn an_expired_lease_deconfigures_and_rediscovers() {
    let mut client = Dhcp::new(MAC, 5);
    bound(&mut client, 0, 100);
    assert_eq!(client.poll(100_000), None, "expiry only resets the client");
    assert_eq!(client.take_event(), Some(Event::Deconfigured));
    assert_eq!(client.lease(), None);
    assert_eq!(
        client.poll(100_000).map(|out| out.kind),
        Some(MessageType::Discover)
    );
}

#[test]
fn a_nak_while_renewing_deconfigures() {
    let mut client = Dhcp::new(MAC, 5);
    bound(&mut client, 0, 1_000);
    let renew = client.poll(500_000).expect("renew");
    client.on_packet(500_050, SERVER, &reply(MessageType::Nak, renew.xid, &[]));
    assert_eq!(client.take_event(), Some(Event::Deconfigured));
}

#[test]
fn invalid_t1_t2_orderings_fall_back_to_the_defaults() {
    let mut client = Dhcp::new(MAC, 5);
    let request = offered(&mut client, 0);
    let mut options = ack_options(1_000);
    options.push((OPT_RENEW, 900u32.to_be_bytes().to_vec()));
    options.push((OPT_REBIND, 400u32.to_be_bytes().to_vec()));
    client.on_packet(
        0,
        SERVER,
        &reply(MessageType::Ack, request.xid, &borrowed(&options)),
    );
    assert!(client.take_event().is_some());
    assert_eq!(client.poll(499_999), None);
    assert!(
        client.poll(500_000).is_some(),
        "T1 defaulted to half the lease"
    );
}

#[test]
fn an_ack_with_a_bad_subnet_mask_is_ignored() {
    let mut client = Dhcp::new(MAC, 5);
    let request = offered(&mut client, 0);
    let mut options = ack_options(1_000);
    options[0].1 = vec![255, 0, 255, 0];
    client.on_packet(
        0,
        SERVER,
        &reply(MessageType::Ack, request.xid, &borrowed(&options)),
    );
    assert_eq!(client.take_event(), None);
    assert_eq!(client.lease(), None);
}

#[test]
fn encoded_messages_carry_the_options_a_server_needs() {
    let out = Outgoing {
        kind: MessageType::Request,
        xid: 0xdead_beef,
        ciaddr: Ipv4Addr::UNSPECIFIED,
        requested: Some(OFFERED),
        server_id: Some(SERVER),
        unicast_to: None,
    };
    let bytes = encode(&out, MAC, 1500);
    assert!(bytes.len() >= MIN_MESSAGE_LEN);
    assert_eq!(&bytes[..4], &[1, 1, 6, 0]);
    assert_eq!(&bytes[4..8], &0xdead_beefu32.to_be_bytes());
    assert_eq!(bytes[10], 0x80, "broadcast flag while unconfigured");
    assert_eq!(&bytes[28..34], &MAC);
    assert_eq!(&bytes[FIXED_LEN..FIXED_LEN + 4], &MAGIC);
    let options = &bytes[FIXED_LEN + 4..];
    assert_eq!(&options[..3], &[OPT_MESSAGE_TYPE, 1, 3]);
    assert!(
        options
            .windows(6)
            .any(|w| w == [OPT_REQUESTED_IP, 4, 192, 168, 1, 50])
    );
    assert!(
        options
            .windows(6)
            .any(|w| w == [OPT_SERVER_ID, 4, 192, 168, 1, 1])
    );
}

#[test]
fn a_renewal_does_not_ask_for_a_broadcast_reply() {
    let out = Outgoing {
        kind: MessageType::Request,
        xid: 1,
        ciaddr: OFFERED,
        requested: None,
        server_id: None,
        unicast_to: Some(SERVER),
    };
    let bytes = encode(&out, MAC, 1500);
    assert_eq!(bytes[10], 0);
    assert_eq!(&bytes[12..16], &OFFERED.octets());
}

#[test]
fn truncated_and_malformed_packets_are_dropped() {
    assert_eq!(parse(&[], MAC), None);
    assert_eq!(parse(&[2; 100], MAC), None);
    let mut bad_magic = reply(MessageType::Offer, 9, &[]);
    bad_magic[FIXED_LEN] = 0;
    assert_eq!(parse(&bad_magic, MAC), None);
    let mut bad_op = reply(MessageType::Offer, 9, &[]);
    bad_op[0] = 1;
    assert_eq!(parse(&bad_op, MAC), None);
    let mut overrun = reply(MessageType::Offer, 9, &[]);
    let last = overrun.len() - 1;
    overrun[last] = OPT_LEASE;
    overrun.push(200);
    assert_eq!(
        parse(&overrun, MAC),
        None,
        "an option longer than the packet"
    );
}
