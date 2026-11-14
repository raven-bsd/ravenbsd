// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ntohs(3)`.
/// The byte-order conversion.
#[unsafe(no_mangle)]
pub extern "C" fn ntohs(value: u16) -> u16 {
    raven_libc_resolv::inet::ntohs(value)
}
