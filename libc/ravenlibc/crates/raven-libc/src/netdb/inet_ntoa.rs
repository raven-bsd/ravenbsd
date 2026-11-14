// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_ntoa(3)`.
use core::ffi::c_char;

use crate::ffi::sockaddr::InAddr;

/// Format one IPv4 address into the `inet_ntoa` scratch area.
#[unsafe(no_mangle)]
pub extern "C" fn inet_ntoa(address: InAddr) -> *mut c_char {
    crate::ffi::tcbbuf::with_area(|area| {
        crate::ffi::map_net::ntop(
            raven_libc_abi::socket::AF_INET,
            &address.s_addr.to_ne_bytes(),
            &mut area.inet_ntoa.bytes,
        )
    })
}
