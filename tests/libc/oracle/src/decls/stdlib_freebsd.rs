// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdlib-freebsd` oracle: AVL tree, hash table, realpath and libgen.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_void};

/// The C `ENTRY`.
#[repr(C)]
pub struct CEntry {
    pub key: *mut c_char,
    pub data: *mut c_void,
}

/// The FreeBSD `struct hsearch_data` (one pointer in the oracle shim).
#[repr(C, align(8))]
pub struct HSearchData {
    pub bytes: [u8; 8],
}

pub type NodeCmp = Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>;
pub type WalkAction = Option<unsafe extern "C" fn(*const c_void, c_int, c_int)>;
pub type KeyFree = Option<unsafe extern "C" fn(*mut c_void)>;

#[cfg_attr(
    feature = "oracle-stdlib-freebsd",
    link(name = "oracle_stdlib_freebsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_tsearch(key: *const c_void, root: *mut *mut c_void, cmp: NodeCmp) -> *mut c_void;
    pub fn oracle_tfind(key: *const c_void, root: *const *mut c_void, cmp: NodeCmp) -> *mut c_void;
    pub fn oracle_tdelete(key: *const c_void, root: *mut *mut c_void, cmp: NodeCmp) -> *mut c_void;
    pub fn oracle_twalk(root: *const c_void, action: WalkAction);
    pub fn oracle_tdestroy(root: *mut c_void, free_key: KeyFree);

    pub fn oracle_hcreate(nel: usize) -> c_int;
    pub fn oracle_hsearch(item: CEntry, action: c_int) -> *mut CEntry;
    pub fn oracle_hdestroy();
    pub fn oracle_hcreate_r(nel: usize, data: *mut HSearchData) -> c_int;
    pub fn oracle_hsearch_r(
        item: CEntry,
        action: c_int,
        retval: *mut *mut CEntry,
        data: *mut HSearchData,
    ) -> c_int;
    pub fn oracle_hdestroy_r(data: *mut HSearchData);

    pub fn oracle_realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
    pub fn oracle___xpg_basename(path: *mut c_char) -> *mut c_char;
    pub fn oracle_dirname(path: *mut c_char) -> *mut c_char;
}
