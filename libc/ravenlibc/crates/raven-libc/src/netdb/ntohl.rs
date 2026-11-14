// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ntohl(3)`.
/// The byte-order conversion.
#[unsafe(no_mangle)]
pub extern "C" fn ntohl(value: u32) -> u32 {
    raven_libc_resolv::inet::ntohl(value)
}
