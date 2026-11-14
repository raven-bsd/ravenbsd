// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getprotoent`, `setprotoent` and `endprotoent`.

use core::ffi::c_int;

use super::records::{self, Protoent};

/// `getprotoent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn getprotoent() -> *mut Protoent {
    records::next_protocol()
        .map(|entry| records::fill_proto(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `setprotoent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn setprotoent(_stayopen: c_int) {
    records::set_protocols();
}

/// `endprotoent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn endprotoent() {
    records::end_protocols();
}
