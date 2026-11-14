// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_lnaof(3)`.
use crate::ffi::sockaddr::InAddr;

/// The host part of an address.
#[unsafe(no_mangle)]
pub extern "C" fn inet_lnaof(address: InAddr) -> u32 {
    raven_libc_resolv::inet_lnaof(address.s_addr)
}
