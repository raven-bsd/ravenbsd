// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getauxval(3)`.
use core::ffi::c_ulong;

use crate::personality_common::aux_lookup;

/// Look up an auxiliary-vector key; an absent key returns 0 and sets `errno = ENOENT`.
#[unsafe(no_mangle)]
pub extern "C" fn getauxval(key: c_ulong) -> c_ulong {
    let (value, error) = raven_libc_personality_linux::auxval::getauxval(key, &aux_lookup);
    if error != 0 {
        crate::ffi::errno::set_raw(error);
    }
    c_ulong::try_from(value).unwrap_or(0)
}
