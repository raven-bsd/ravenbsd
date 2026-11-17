// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hsearch`.

use core::ffi::c_int;

use raven_libc_abi::errno::Errno;

use super::search::CEntry;

/// `hsearch`.
///
/// # Safety
/// The `ENTRY` is passed by value; the returned pointer is valid until the next call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hsearch(item: CEntry, action: c_int) -> *mut CEntry {
    // SAFETY: the global slot is the one `hcreate` wrote.
    let slot = unsafe { &mut *super::search::GLOBAL.0.get() };
    let Some(table) = slot.as_mut() else {
        crate::ffi::errno::set(Errno(22));
        return core::ptr::null_mut();
    };
    let action = super::search::action_of(action);
    super::search::search(table, &item, action, &super::search::GLOBAL_OUT)
}
