// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setnetent` and `endnetent`.

use core::ffi::c_int;

use super::records;

/// `setnetent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn setnetent(_stayopen: c_int) {
    records::set_networks();
}

/// `endnetent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn endnetent() {
    records::end_networks();
}
