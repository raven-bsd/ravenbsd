// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Platform backend tests: the netd control reply is a stream, not one record.

use super::*;
use crate::ErrnoExt;
use raven_proto_net::{IF_UP, IfName, IfaceInfo};

fn iface(name: &str, addr: [u8; 4]) -> IfaceInfo {
    IfaceInfo {
        name: IfName::new(name).expect("name"),
        addr,
        prefix: 24,
        flags: IF_UP,
        mtu: 1500,
        mac: [0x52, 0x54, 0, 0x12, 0x34, 0x56],
    }
}

fn encode(records: &[IfaceResponse]) -> Vec<u8> {
    let mut buffer = alloc::vec![0u8; 512];
    let mut at = 0usize;
    for record in records {
        let used = record.encode(&mut buffer[at..]).expect("encode");
        at = at.saturating_add(used);
    }
    buffer.truncate(at);
    buffer
}

#[test]
fn list_stream_is_split_into_its_records() {
    // netd packs `Count` then one `Iface` per interface into a single Call reply.
    let buffer = encode(&[
        IfaceResponse::Count { count: 2 },
        IfaceResponse::Iface(iface("lo", [127, 0, 0, 1])),
        IfaceResponse::Iface(iface("eth0", [10, 0, 2, 15])),
    ]);
    let records = split_responses(&buffer).expect("stream");
    assert_eq!(records.len(), 3);
    assert_eq!(records[0], IfaceResponse::Count { count: 2 });
    assert!(matches!(records[1], IfaceResponse::Iface(info) if info.name.as_str() == "lo"));
    assert!(matches!(records[2], IfaceResponse::Iface(info) if info.name.as_str() == "eth0"));
}

#[test]
fn a_truncated_stream_is_eproto_not_a_partial_list() {
    let mut buffer = encode(&[
        IfaceResponse::Count { count: 1 },
        IfaceResponse::Iface(iface("eth0", [10, 0, 2, 15])),
    ]);
    buffer.pop();
    assert_eq!(split_responses(&buffer), Err(Errno::EPROTO));
}

#[test]
fn a_single_record_still_splits() {
    let buffer = encode(&[IfaceResponse::Count { count: 0 }]);
    assert_eq!(
        split_responses(&buffer),
        Ok(alloc::vec![IfaceResponse::Count { count: 0 }])
    );
}
