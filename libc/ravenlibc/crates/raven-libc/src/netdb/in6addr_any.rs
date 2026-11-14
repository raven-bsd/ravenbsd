// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `in6addr_any` data symbol.
use crate::ffi::sockaddr::In6Addr;

/// The all-zero IPv6 address.
#[unsafe(no_mangle)]
pub static in6addr_any: In6Addr = In6Addr { s6_addr: [0; 16] };
