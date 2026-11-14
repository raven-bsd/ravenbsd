// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_makeaddr(3)`.
use crate::ffi::sockaddr::InAddr;

/// Combine a network and host number.
#[unsafe(no_mangle)]
pub extern "C" fn inet_makeaddr(network: u32, host: u32) -> InAddr {
    InAddr {
        s_addr: raven_libc_resolv::inet_makeaddr(network, host),
    }
}
