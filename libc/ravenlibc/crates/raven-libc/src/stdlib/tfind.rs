// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tfind(3)`.
use core::ffi::{c_int, c_void};

use super::search_support::{RawNodes, comparator, root_from};

/// Find `key` in the tree at `rootp` without modifying it.
///
/// # Safety
/// `rootp` is readable; `compar` is a valid C comparator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tfind(
    key: *const c_void,
    rootp: *const *mut c_void,
    compar: Option<extern "C" fn(*const c_void, *const c_void) -> c_int>,
) -> *mut c_void {
    if rootp.is_null() {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    }
    // SAFETY: the caller's root word.
    let root = root_from(unsafe { *rootp });
    let store = RawNodes;
    let mut cmp = comparator(compar);
    raven_libc_stdlib::search::tfind(&store, root, key as usize, &mut cmp)
        .map_or(core::ptr::null_mut(), |node| node.cast::<c_void>())
}
