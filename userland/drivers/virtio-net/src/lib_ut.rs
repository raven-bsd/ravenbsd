// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the virtio-net core: layout, descriptor encoding, headers and ARP.

extern crate alloc;

use alloc::string::ToString;

use super::*;

#[test]
fn a_queue_layout_is_aligned_and_bounded() {
    let layout = RingLayout::new(0x120, 8);
    assert_eq!(layout.desc % 16, 0);
    assert_eq!(layout.avail % 2, 0);
    assert_eq!(layout.used % 4, 0);
    assert!(layout.desc < layout.avail);
    assert!(layout.avail < layout.used);
    assert_eq!(layout.end, layout.used + 4 + 8 * 8);
    assert!(layout.desc >= 0x120);
}

#[test]
fn queue_slots_cover_every_entry_and_nothing_else() {
    let layout = RingLayout::new(0, 8);
    assert_eq!(layout.avail_slot(0), Some(layout.avail + 4));
    assert_eq!(layout.avail_slot(7), Some(layout.avail + 4 + 14));
    assert_eq!(layout.avail_slot(8), None);
    assert_eq!(layout.used_slot(0), Some(layout.used + 4));
    assert_eq!(layout.used_slot(7), Some(layout.used + 4 + 56));
    assert_eq!(layout.used_slot(8), None);
    assert_eq!(layout.avail_idx(), layout.avail + 2);
    assert_eq!(layout.used_idx(), layout.used + 2);
}

#[test]
fn a_layout_rounds_an_unaligned_base_up() {
    let layout = RingLayout::new(0x21, 4);
    assert_eq!(layout.desc, 0x30);
}

#[test]
fn queue_size_is_a_power_of_two_within_the_limit() {
    assert_eq!(queue_size(0), None);
    assert_eq!(queue_size(3), None);
    assert_eq!(queue_size(4), Some(4));
    assert_eq!(queue_size(7), Some(4));
    assert_eq!(queue_size(8), Some(8));
    assert_eq!(queue_size(9), Some(8));
    assert_eq!(queue_size(256), Some(8));
}

#[test]
fn a_descriptor_round_trips() {
    let descriptor = Descriptor {
        addr: 0x1_2345_6789,
        len: 1514,
        flags: DESC_F_WRITE,
        next: 3,
    };
    let encoded = encode_descriptor(descriptor);
    assert_eq!(decode_descriptor(&encoded), Some(descriptor));
    assert_eq!(decode_descriptor(&encoded[..15]), None);
}

#[test]
fn a_used_entry_round_trips() {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&2u32.to_le_bytes());
    bytes[4..8].copy_from_slice(&42u32.to_le_bytes());
    assert_eq!(decode_used(&bytes), Some(UsedEntry { id: 2, len: 42 }));
    assert_eq!(decode_used(&bytes[..7]), None);
}

#[test]
fn the_net_header_round_trips() {
    let header = NetHeader {
        flags: 0,
        gso_type: 0,
        hdr_len: 14,
        gso_size: 0,
        csum_start: 0,
        csum_offset: 0,
    };
    let bytes = header.encode();
    assert_eq!(bytes.len(), NET_HEADER_LEN);
    assert_eq!(NetHeader::decode(&bytes), Some(header));
    assert_eq!(NetHeader::decode(&bytes[..11]), None);
}

#[test]
fn a_zero_header_is_twelve_zero_bytes() {
    assert_eq!(NetHeader::default().encode(), [0u8; NET_HEADER_LEN]);
}

#[test]
fn an_arp_request_has_the_expected_wire_fields() {
    let mac = MacAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let mut frame = [0u8; 42];
    let length = build_arp_request(mac, [10, 0, 2, 15], [10, 0, 2, 2], &mut frame);
    assert_eq!(length, Some(42));
    assert_eq!(&frame[0..6], &[0xFF; 6]);
    assert_eq!(&frame[6..12], &mac.0);
    assert_eq!(&frame[12..14], &[0x08, 0x06]);
    assert_eq!(&frame[14..16], &[0x00, 0x01]);
    assert_eq!(&frame[16..18], &[0x08, 0x00]);
    assert_eq!(frame[18], 6);
    assert_eq!(frame[19], 4);
    assert_eq!(&frame[20..22], &[0x00, 0x01]);
    assert_eq!(&frame[28..32], &[10, 0, 2, 15]);
    assert_eq!(&frame[38..42], &[10, 0, 2, 2]);
    assert!(build_arp_request(mac, [0; 4], [0; 4], &mut [0u8; 41]).is_none());
}

fn reply_for(request: &[u8], sender_mac: [u8; 6], sender_ip: [u8; 4]) -> [u8; 42] {
    let mut reply = [0u8; 42];
    reply.copy_from_slice(request);
    reply[0..6].copy_from_slice(&request[6..12]); // to the requester
    reply[6..12].copy_from_slice(&sender_mac);
    reply[20..22].copy_from_slice(&2u16.to_be_bytes()); // reply
    reply[22..28].copy_from_slice(&sender_mac);
    reply[28..32].copy_from_slice(&sender_ip);
    reply[32..38].copy_from_slice(&request[22..28]);
    reply[38..42].copy_from_slice(&request[28..32]);
    reply
}

#[test]
fn an_arp_reply_parses() {
    let mac = MacAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let mut request = [0u8; 42];
    build_arp_request(mac, [10, 0, 2, 15], [10, 0, 2, 2], &mut request).unwrap();
    let gateway = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];
    let reply = reply_for(&request, gateway, [10, 0, 2, 2]);
    let parsed = parse_arp_reply(&reply).unwrap();
    assert_eq!(parsed.sender_mac.0, gateway);
    assert_eq!(parsed.sender_ip, [10, 0, 2, 2]);
    assert_eq!(parsed.target_ip, [10, 0, 2, 15]);
}

#[test]
fn malformed_arp_frames_are_rejected() {
    let mac = MacAddress([1, 2, 3, 4, 5, 6]);
    let mut request = [0u8; 42];
    build_arp_request(mac, [10, 0, 2, 15], [10, 0, 2, 2], &mut request).unwrap();
    let reply = reply_for(&request, mac.0, [10, 0, 2, 2]);
    assert!(parse_arp_reply(&reply).is_some());
    assert!(parse_arp_reply(&reply[..41]).is_none());

    let mut wrong_ethertype = reply;
    wrong_ethertype[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    assert!(parse_arp_reply(&wrong_ethertype).is_none());

    let mut request_op = reply;
    request_op[20..22].copy_from_slice(&1u16.to_be_bytes());
    assert!(parse_arp_reply(&request_op).is_none());

    let mut wrong_hlen = reply;
    wrong_hlen[18] = 8;
    assert!(parse_arp_reply(&wrong_hlen).is_none());
}

#[test]
fn a_mac_address_displays_in_dotted_hex() {
    let mac = MacAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    assert_eq!(mac.to_string(), "52:54:00:12:34:56");
}

#[test]
fn feature_words_are_queried_by_absolute_bit() {
    let features = Features {
        low: NET_F_MAC | NET_F_MTU,
        high: F_VERSION_1,
    };
    assert!(features.has(5));
    assert!(features.has(3));
    assert!(features.has(32));
    assert!(!features.has(16));
    assert!(!features.has(64));
}

#[test]
fn the_control_summary_round_trips() {
    let info = ControlInfo {
        mac: MacAddress([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]),
        mtu: 1500,
        link: true,
        dma_len: 65536,
        ring_offset: RING_REGION_OFFSET,
        ring_len: RING_REGION_LEN,
        rx_count: RING_RX_COUNT,
        tx_count: RING_TX_COUNT,
        pool_len: RING_REGION_LEN - 2048,
    };
    let mut text = [0u8; 256];
    let length = info.encode(&mut text).unwrap();
    let decoded = ControlInfo::decode(core::str::from_utf8(&text[..length]).unwrap()).unwrap();
    assert_eq!(decoded, info);

    let mut down = info;
    down.link = false;
    let length = down.encode(&mut text).unwrap();
    assert_eq!(
        ControlInfo::decode(core::str::from_utf8(&text[..length]).unwrap()),
        Some(down)
    );
}

#[test]
fn the_control_summary_rejects_bad_input() {
    let good = "mac=52:54:00:12:34:56\nmtu=1500\nlink=up\nring_offset=4096\nring_len=24576\nrx=8\ntx=8\npool=22528\ndma=65536\n";
    assert!(ControlInfo::decode(good).is_some());
    assert!(ControlInfo::decode(&good.replace("link=up", "link=sideways")).is_none());
    assert!(ControlInfo::decode(&good.replace("mtu=1500\n", "")).is_none());
    assert!(ControlInfo::decode(&good.replace("rx=8", "extra=1")).is_none());
    assert!(ControlInfo::decode(&good.replace("mac=52:54:00:12:34:56", "mac=zz")).is_none());
}

#[test]
fn mac_addresses_parse_strictly() {
    assert_eq!(
        parse_mac("52:54:00:ab:cd:ef"),
        Some(MacAddress([0x52, 0x54, 0x00, 0xab, 0xcd, 0xef]))
    );
    assert_eq!(parse_mac("52:54:00:ab:cd"), None);
    assert_eq!(parse_mac("52-54-00-ab-cd-ef"), None);
    assert_eq!(parse_mac("52:54:00:ab:cd:g0"), None);
}

#[test]
fn the_b39_region_layout_matches_the_contract() {
    let layout = ring_layout().unwrap();
    assert_eq!(layout.region_len(), RING_REGION_LEN);
    assert_eq!(layout.pool_offset(), 2048);
    assert_eq!(layout.buffer_count(), 11);
    assert_eq!(layout.count(raven_proto_net::ring::Ring::Rx), 8);
    assert_eq!(layout.count(raven_proto_net::ring::Ring::Tx), 8);
}

#[test]
fn a_tx_descriptor_must_name_a_complete_pool_frame() {
    let layout = ring_layout().unwrap();
    let mut region = [0u8; RING_REGION_LEN];
    let buffer = layout.buffer_offset(8).unwrap();
    region[buffer] = 0x42;
    let good = netring::Desc::new(buffer as u64, 1, netring::DESC_FIRST | netring::DESC_LAST);
    assert_eq!(
        frame_desc(&region, &layout, good),
        Some(&region[buffer..buffer + 1])
    );

    let error = netring::Desc::new(
        buffer as u64,
        1,
        netring::DESC_FIRST | netring::DESC_LAST | netring::DESC_ERROR,
    );
    assert_eq!(frame_desc(&region, &layout, error), None);
    let header_only = netring::Desc::new(buffer as u64, 1, netring::DESC_FIRST);
    assert_eq!(frame_desc(&region, &layout, header_only), None);
    let outside = netring::Desc::new((layout.pool_offset() - 1) as u64, 1, 3);
    assert_eq!(frame_desc(&region, &layout, outside), None);
    let too_long = netring::Desc::new(buffer as u64, 4096, 3);
    assert_eq!(frame_desc(&region, &layout, too_long), None);
}

#[test]
fn an_rx_descriptor_points_after_the_virtio_header() {
    let layout = ring_layout().unwrap();
    let desc = rx_desc(&layout, 0, 64).unwrap();
    assert_eq!(
        desc,
        netring::Desc::new(
            (layout.buffer_offset(0).unwrap() + VIRTIO_NET_HEADER) as u64,
            64,
            3
        )
    );
    assert!(rx_desc(&layout, RING_RX_BUFFERS, 64).is_none());
    assert!(rx_desc(&layout, 0, 0).is_none());
    assert!(rx_desc(&layout, 0, MAX_FRAME_LEN + 1).is_none());
}
