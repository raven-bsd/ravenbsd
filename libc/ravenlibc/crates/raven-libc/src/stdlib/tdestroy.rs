// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tdestroy(3)`.
use core::ffi::c_void;

use super::search_support::{RawNodes, root_from};

/// Free every node of the tree at `root`, calling `free_key` per key first.
///
/// # Safety
/// `root` is a tree root or NULL; `free_key` is NULL or a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tdestroy(root: *mut c_void, free_key: Option<extern "C" fn(*mut c_void)>) {
    let mut store = RawNodes;
    let mut release = |key: usize| {
        if let Some(func) = free_key {
            func(key as *mut c_void);
        }
    };
    raven_libc_stdlib::search::tdestroy(&mut store, root_from(root), &mut release);
}
