// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;
use raven_libc_net::iface::{IFF_BROADCAST, IFF_UP};

fn sample() -> IfAddr {
    IfAddr {
        name: b"eth0".to_vec(),
        flags: IFF_UP | IFF_BROADCAST,
        addr: [10, 0, 0, 1],
        netmask: [255, 255, 255, 0],
        broadaddr: Some([10, 0, 0, 255]),
    }
}

#[test]
fn list_roundtrips_the_interface_record() {
    let items = [sample()];
    let head = build_list(&items).expect("list");
    assert!(!head.is_null());
    // SAFETY: `head` is the live node built above.
    let node = unsafe { &*head };
    assert_eq!(node.ifa_flags, IFF_UP | IFF_BROADCAST);
    assert!(!node.ifa_name.is_null());
    // SAFETY: the name came from a NUL-terminated CString.
    let name = unsafe { core::ffi::CStr::from_ptr(node.ifa_name) };
    assert_eq!(name.to_bytes(), b"eth0");
    // SAFETY: `ifa_addr` is a 16-byte sockaddr_in.
    let address = unsafe { core::slice::from_raw_parts(node.ifa_addr.cast::<u8>(), 16) };
    assert_eq!(u16::from_ne_bytes([address[0], address[1]]), 2);
    assert_eq!(&address[4..8], &[10, 0, 0, 1]);
    // SAFETY: `ifa_netmask` is a 16-byte sockaddr_in.
    let mask = unsafe { core::slice::from_raw_parts(node.ifa_netmask.cast::<u8>(), 16) };
    assert_eq!(&mask[4..8], &[255, 255, 255, 0]);
    // SAFETY: the union was initialised with broadaddr.
    let broad = unsafe { node.ifa_ifu.broadaddr };
    assert!(!broad.is_null());
    // SAFETY: broad is a 16-byte sockaddr_in.
    let broadcast = unsafe { core::slice::from_raw_parts(broad.cast::<u8>(), 16) };
    assert_eq!(&broadcast[4..8], &[10, 0, 0, 255]);
    // SAFETY: the list is freed exactly once here.
    unsafe { free_list(head) };
}

#[test]
fn addresses_use_the_shared_socket_writer() {
    let items = [sample()];
    let head = build_list(&items).expect("list");
    let mut expected = [0u8; 16];
    let full = raven_libc_net::addr::write_sockaddr(
        &NetSockAddr::In {
            ip: [10, 0, 0, 1],
            port: 0,
        },
        &mut expected,
    );
    assert_eq!(full, 16);
    // SAFETY: `head` is the live node built above; `ifa_addr` is a 16-byte sockaddr_in.
    let node = unsafe { &*head };
    // SAFETY: `ifa_addr` points at the 16 bytes written by the shared writer.
    let address = unsafe { core::slice::from_raw_parts(node.ifa_addr.cast::<u8>(), full) };
    assert_eq!(address, &expected);
    // SAFETY: the list is freed exactly once here.
    unsafe { free_list(head) };
}

#[test]
fn malformed_name_fails_the_list_without_a_partial_output() {
    let mut bad = sample();
    bad.name = b"bad\0name".to_vec();
    let items = [sample(), bad];
    let result = build_list(&items);
    assert_eq!(result.err(), Some(raven_libc_abi::errno::Errno::ENOMEM));
}

#[test]
fn null_output_is_rejected_without_touching_the_net() {
    // SAFETY: the wrapper checks the NULL output before any backend call.
    let result = unsafe { getifaddrs(core::ptr::null_mut()) };
    assert_eq!(result, -1);
    assert_eq!(
        crate::ffi::errno::get(),
        i32::from(raven_libc_abi::errno::Errno::EINVAL.0)
    );
}
