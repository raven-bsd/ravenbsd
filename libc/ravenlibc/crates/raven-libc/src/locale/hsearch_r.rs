// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hsearch_r`.

use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::search::HTable;

use super::search::{CEntry, HsearchData};

/// `hsearch_r`.
///
/// # Safety
/// `retval` is writable; `data` is NULL or the table of a successful `hcreate_r`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hsearch_r(
    item: CEntry,
    action: c_int,
    retval: *mut *mut CEntry,
    data: *mut HsearchData,
) -> c_int {
    if retval.is_null() || data.is_null() {
        crate::ffi::errno::set(Errno(22));
        return 0;
    }
    // SAFETY: the caller's struct; `tab` is a Box from `hcreate_r`.
    let tab = unsafe { (*data).tab };
    if tab.is_null() {
        crate::ffi::errno::set(Errno(22));
        return 0;
    }
    // SAFETY: `tab` is a live boxed table owned by the caller.
    let table = unsafe { &mut *tab.cast::<HTable>() };
    let action = super::search::action_of(action);
    let result = super::search::search(table, &item, action, &super::search::DATA_OUT);
    // SAFETY: the caller's out pointer.
    unsafe { *retval = result };
    c_int::from(!result.is_null())
}
