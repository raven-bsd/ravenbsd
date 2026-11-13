// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interface tests over the fake control channel.

use super::*;
use crate::ErrnoExt;
use crate::fake::{FakeNetd, record};
use raven_proto_net::{IF_LINK_UP, IF_LOOPBACK, IF_UP};

fn fake() -> FakeNetd {
    FakeNetd::new()
}

#[test]
fn name_to_index_and_back() {
    let fake = fake();
    assert_eq!(nametoindex(&fake, b"lo"), Ok(1));
    assert_eq!(nametoindex(&fake, b"eth0"), Ok(2));
    let mut name = [0u8; MAX_IFNAME];
    assert_eq!(indextoname(&fake, 1, &mut name), Ok(2));
    assert_eq!(&name[..3], b"lo\0");
    assert_eq!(indextoname(&fake, 2, &mut name), Ok(4));
    assert_eq!(&name[..5], b"eth0\0");
}

#[test]
fn unknown_names_and_indices() {
    let fake = fake();
    assert_eq!(nametoindex(&fake, b"missing"), Err(Errno::ENODEV));
    assert_eq!(nametoindex(&fake, b"a234567890123456"), Err(Errno::ENODEV));
    let mut name = [0u8; MAX_IFNAME];
    assert_eq!(indextoname(&fake, 0, &mut name), Err(Errno::ENXIO));
    assert_eq!(indextoname(&fake, 99, &mut name), Err(Errno::ENXIO));
}

#[test]
fn nameindex_order() {
    let fake = fake();
    assert_eq!(
        nameindex(&fake),
        Ok(vec![(1, b"lo".to_vec()), (2, b"eth0".to_vec())])
    );
}

#[test]
fn getifaddrs_flags_masks_and_broadcast() {
    let fake = fake();
    let list = getifaddrs(&fake).expect("getifaddrs");
    assert_eq!(list.len(), 2);
    let lo = &list[0];
    assert_eq!(lo.name, b"lo");
    assert_ne!(lo.flags & IFF_LOOPBACK, 0);
    assert_ne!(lo.flags & IFF_UP, 0);
    assert_ne!(lo.flags & IFF_RUNNING, 0);
    assert!(lo.broadaddr.is_none());
    assert_eq!(lo.netmask, [255, 0, 0, 0]);

    let eth0 = &list[1];
    assert_eq!(eth0.netmask, [255, 255, 255, 0]);
    assert_eq!(eth0.broadaddr, Some([10, 0, 2, 255]));
    assert_ne!(eth0.flags & IFF_BROADCAST, 0);
    assert_ne!(eth0.flags & IFF_MULTICAST, 0);
}

#[test]
fn prefix_masks() {
    assert_eq!(prefix_mask(0), [0, 0, 0, 0]);
    assert_eq!(prefix_mask(8), [255, 0, 0, 0]);
    assert_eq!(prefix_mask(24), [255, 255, 255, 0]);
    assert_eq!(prefix_mask(31), [255, 255, 255, 254]);
    assert_eq!(prefix_mask(32), [255, 255, 255, 255]);
}

#[test]
fn broadcast_arithmetic() {
    assert_eq!(
        broadcast([10, 0, 2, 15], [255, 255, 255, 0]),
        [10, 0, 2, 255]
    );
    assert_eq!(
        broadcast([192, 0, 2, 1], [255, 255, 255, 254]),
        [192, 0, 2, 1]
    );
}

#[test]
fn no_running_bit_without_link() {
    let fake = FakeNetd::new();
    let info = record("eth9", [10, 9, 0, 1], 32, IF_UP, 1500, [0; 6]).expect("record");
    fake.set_ifaces(vec![info]);
    let list = getifaddrs(&fake).expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].flags & IFF_RUNNING, 0);
    assert_eq!(list[0].flags & IFF_LOOPBACK, 0);
    assert_eq!(list[0].broadaddr, None);
    assert_eq!(list[0].netmask, [255, 255, 255, 255]);
}

#[test]
fn ifindex_capability_is_honoured() {
    let fake = FakeNetd::new();
    let mut info = record(
        "lo",
        [127, 0, 0, 1],
        8,
        IF_UP | IF_LINK_UP | IF_LOOPBACK,
        65_535,
        [0; 6],
    )
    .expect("record");
    info.flags |= IF_LINK_UP;
    fake.set_ifaces(vec![info]);
    assert_eq!(nametoindex(&fake, b"lo"), Ok(1));
}

#[test]
fn point_to_point_prefixes_have_no_broadcast() {
    let fake = FakeNetd::new();
    let a = record("p31", [10, 3, 0, 1], 31, IF_UP | IF_LINK_UP, 1500, [0; 6]).expect("a");
    let b = record("p32", [10, 4, 0, 1], 32, IF_UP | IF_LINK_UP, 1500, [0; 6]).expect("b");
    fake.set_ifaces(vec![a, b]);
    let list = getifaddrs(&fake).expect("list");
    assert!(list[0].broadaddr.is_none());
    assert!(list[1].broadaddr.is_none());
    assert_eq!(list[0].netmask, [255, 255, 255, 254]);
    assert_eq!(list[1].netmask, [255, 255, 255, 255]);
}

#[test]
fn sixteen_byte_names_are_unknown() {
    let fake = FakeNetd::new();
    assert_eq!(nametoindex(&fake, b"abcdefghijklmnop"), Err(Errno::ENODEV));
    let mut name = [0u8; MAX_IFNAME];
    assert_eq!(indextoname(&fake, 99, &mut name), Err(Errno::ENXIO));
}
