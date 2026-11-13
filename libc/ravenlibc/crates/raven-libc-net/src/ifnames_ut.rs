// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Resolver `IfNames` tests.

use super::*;
use crate::fake::{FakeNetd, record};
use raven_libc_resolv::IfNames;
use raven_proto_net::{IF_LINK_UP, IF_LOOPBACK, IF_UP};

fn net(caps: crate::Caps) -> Net<FakeNetd, FakeNetd, FakeNetd> {
    let fake = FakeNetd::with_caps(caps);
    let mut first = record(
        "lo",
        [127, 0, 0, 1],
        8,
        IF_UP | IF_LINK_UP | IF_LOOPBACK,
        65_535,
        [0; 6],
    )
    .expect("lo");
    let mut second =
        record("eth0", [10, 0, 2, 15], 24, IF_UP | IF_LINK_UP, 1500, [0; 6]).expect("eth0");
    first.flags |= IF_LINK_UP;
    second.flags |= IF_LINK_UP;
    fake.set_ifaces(vec![first, second]);
    Net::new(fake.clone(), fake.clone(), fake.clone())
}

#[test]
fn name_and_index_round_trip() {
    let net = net(crate::Caps::default());
    let names = net.ifnames();
    assert_eq!(names.name_to_index("lo"), Some(1));
    assert_eq!(names.name_to_index("eth0"), Some(2));
    assert_eq!(names.name_to_index("missing"), None);
    assert_eq!(names.index_to_name(1).as_deref(), Some("lo"));
    assert_eq!(names.index_to_name(2).as_deref(), Some("eth0"));
    assert_eq!(names.index_to_name(99), None);
}

#[test]
fn ifindex_capability_keeps_the_same_round_trip() {
    let caps = crate::Caps {
        ifindex: true,
        ..crate::Caps::default()
    };
    let net = net(caps);
    let names = net.ifnames();
    assert_eq!(names.name_to_index("eth0"), Some(2));
    assert_eq!(names.index_to_name(2).as_deref(), Some("eth0"));
}

#[test]
fn long_names_are_rejected() {
    let net = net(crate::Caps::default());
    assert_eq!(net.ifnames().name_to_index("abcdefghijklmnop"), None);
}
