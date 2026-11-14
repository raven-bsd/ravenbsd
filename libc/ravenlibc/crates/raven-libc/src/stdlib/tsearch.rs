// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tsearch(3)`.
use core::ffi::{c_int, c_void};

use super::search_support::{RawNodes, comparator, root_from, root_word};

/// Insert `key` into the tree at `rootp` and return its node.
///
/// # Safety
/// `rootp` is writable; `compar` is a valid C comparator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tsearch(
    key: *const c_void,
    rootp: *mut *mut c_void,
    compar: Option<extern "C" fn(*const c_void, *const c_void) -> c_int>,
) -> *mut c_void {
    if rootp.is_null() {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
    }
    // SAFETY: the caller's root word.
    let mut root = root_from(unsafe { *rootp });
    let mut store = RawNodes;
    let mut cmp = comparator(compar);
    let node = raven_libc_stdlib::search::tsearch(&mut store, &mut root, key as usize, &mut cmp);
    // SAFETY: the caller's writable root word.
    unsafe { *rootp = root_word(root) };
    node.map_or(core::ptr::null_mut(), |node| node.cast::<c_void>())
}
