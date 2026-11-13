// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Route approximation tests.

use super::*;
use crate::fake::{FakeNetd, record};
use raven_proto_net::IF_LINK_UP;

fn iface(name: &str, addr: [u8; 4], prefix: u8, flags: u8) -> IfaceInfo {
    record(name, addr, prefix, flags, 1500, [0; 6]).expect("iface")
}

#[test]
fn loopback_destination_prefers_loopback() {
    let list = vec![
        iface("lo", [127, 0, 0, 1], 8, IF_UP | IF_LINK_UP | IF_LOOPBACK),
        iface("eth0", [10, 0, 2, 15], 24, IF_UP | IF_LINK_UP),
    ];
    assert_eq!(
        approximation(&list, [127, 0, 0, 5]),
        Some(IpAddr::V4([127, 0, 0, 1]))
    );
}

#[test]
fn on_link_uses_the_interface_address() {
    let list = vec![
        iface("lo", [127, 0, 0, 1], 8, IF_UP | IF_LINK_UP | IF_LOOPBACK),
        iface("eth0", [10, 0, 2, 15], 24, IF_UP | IF_LINK_UP),
    ];
    assert_eq!(
        approximation(&list, [10, 0, 2, 99]),
        Some(IpAddr::V4([10, 0, 2, 15]))
    );
}

#[test]
fn off_link_uses_the_default_interface() {
    let list = vec![
        iface("lo", [127, 0, 0, 1], 8, IF_UP | IF_LINK_UP | IF_LOOPBACK),
        iface("eth0", [10, 0, 2, 15], 24, IF_UP | IF_LINK_UP),
    ];
    assert_eq!(
        approximation(&list, [192, 0, 2, 1]),
        Some(IpAddr::V4([10, 0, 2, 15]))
    );
}

#[test]
fn no_usable_interface_is_none() {
    let list = vec![iface("eth0", [10, 0, 2, 15], 24, IF_UP)];
    assert_eq!(approximation(&list, [192, 0, 2, 1]), None);
    assert_eq!(approximation(&list, [127, 0, 0, 1]), None);
    assert_eq!(approximation(&[], [10, 0, 0, 1]), None);
}

#[test]
fn family_usable_requires_up_and_link() {
    let fake = FakeNetd::new();
    let net = crate::Net::new(
        fake,
        crate::fake::FakeNetd::new(),
        crate::fake::FakeNetd::new(),
    );
    let _ = net;
    let list = vec![
        iface("lo", [127, 0, 0, 1], 8, IF_UP | IF_LINK_UP | IF_LOOPBACK),
        iface("eth1", [10, 1, 0, 1], 24, IF_UP),
    ];
    assert_eq!(approximation(&list, [10, 1, 0, 2]), None);
}

#[test]
fn family_usable_v6_needs_the_capability() {
    use raven_libc_resolv::{Family, Probe};
    let fake = crate::fake::FakeNetd::new();
    let net = crate::Net::new(fake.clone(), fake.clone(), fake.clone());
    assert!(!net.probe().family_usable(Family::V6));
    assert!(net.probe().family_usable(Family::V4));
}
