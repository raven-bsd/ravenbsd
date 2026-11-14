// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getservent`, `setservent` and `endservent`.

use core::ffi::c_int;

use super::records::{self, Servent};

/// `getservent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn getservent() -> *mut Servent {
    records::next_service()
        .map(|entry| records::fill_serv(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `setservent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn setservent(_stayopen: c_int) {
    records::set_services();
}

/// `endservent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn endservent() {
    records::end_services();
}
