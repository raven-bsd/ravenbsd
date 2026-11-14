// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getisax` (sys/auxv.h).

use core::ffi::c_uint;

use raven_libc_personality_solaris::identity;

use crate::personality_solaris::real::RealAux;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getisax(array: *mut u32, n: c_uint) -> c_uint {
    if array.is_null() || n == 0 {
        return 0;
    }
    // SAFETY: the caller passed `n` writable `uint32_t` elements (C prototype).
    let out = unsafe { core::slice::from_raw_parts_mut(array, n as usize) };
    c_uint::try_from(identity::getisax(&RealAux, out)).unwrap_or(0)
}
