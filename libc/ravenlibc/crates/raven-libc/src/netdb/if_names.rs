// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared C layout for the `<net/if.h>` exports.

use core::ffi::{c_char, c_uint};

/// The C `struct if_nameindex`.
#[repr(C)]
pub struct CIfNameIndex {
    /// Interface index.
    pub if_index: c_uint,
    /// Interface name.
    pub if_name: *mut c_char,
}
