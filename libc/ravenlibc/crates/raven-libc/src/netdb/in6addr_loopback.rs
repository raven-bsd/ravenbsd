// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `in6addr_loopback` data symbol.
use crate::ffi::sockaddr::In6Addr;

/// The IPv6 loopback address (`::1`).
#[unsafe(no_mangle)]
pub static in6addr_loopback: In6Addr = In6Addr {
    s6_addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
};
