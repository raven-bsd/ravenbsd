// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `htons(3)`.
/// The byte-order conversion.
#[unsafe(no_mangle)]
pub extern "C" fn htons(value: u16) -> u16 {
    raven_libc_resolv::inet::htons(value)
}
