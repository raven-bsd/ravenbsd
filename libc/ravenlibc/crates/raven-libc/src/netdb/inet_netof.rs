// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_netof(3)`.
use crate::ffi::sockaddr::InAddr;

/// The network part of an address.
#[unsafe(no_mangle)]
pub extern "C" fn inet_netof(address: InAddr) -> u32 {
    raven_libc_resolv::inet_netof(address.s_addr)
}
