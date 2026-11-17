// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared state for the `<search.h>` hash-table exports.

use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_uint, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_stdlib::search::{
    Action, Entropy, Entry as HashEntry, HSearchError, HTable, KeyText, hcreate as make_table,
};

/// The C `ENTRY`.
#[repr(C)]
pub(crate) struct CEntry {
    /// The caller's key pointer.
    pub(crate) key: *mut c_char,
    /// The caller's data pointer.
    pub(crate) data: *mut c_void,
}

/// The C `struct hsearch_data`; only `__tab` is used.
#[repr(C)]
pub(crate) struct HsearchData {
    /// The boxed `HTable`.
    pub(crate) tab: *mut c_void,
    /// Padding word.
    pub(crate) unused1: c_uint,
    /// Padding word.
    pub(crate) unused2: c_uint,
}

/// The C-string key seam.
struct CKeys;

impl KeyText for CKeys {
    fn text(&self, key: usize) -> &[u8] {
        // SAFETY: the words in a table are the caller's NUL-terminated keys.
        unsafe { crate::ffi::cstr::slice(key as *const c_char) }
    }
}

/// A deterministic per-process hash basis.
struct ShellEntropy;

impl Entropy for ShellEntropy {
    fn fill(&mut self, buf: &mut [u8]) {
        let mut value = 0x9e37_79b9u32;
        for byte in buf.iter_mut() {
            value = value.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = value as u8;
        }
    }
}

pub(crate) struct GlobalTable(pub(crate) UnsafeCell<Option<HTable>>);

// SAFETY: the shell is single-threaded until C57; the C contract serialises the calls.
unsafe impl Sync for GlobalTable {}

pub(crate) static GLOBAL: GlobalTable = GlobalTable(UnsafeCell::new(None));

pub(crate) struct Output(pub(crate) UnsafeCell<CEntry>);

// SAFETY: one C call at a time, as the `hsearch` contract requires.
unsafe impl Sync for Output {}

pub(crate) static GLOBAL_OUT: Output = Output(UnsafeCell::new(CEntry {
    key: core::ptr::null_mut(),
    data: core::ptr::null_mut(),
}));

pub(crate) static DATA_OUT: Output = Output(UnsafeCell::new(CEntry {
    key: core::ptr::null_mut(),
    data: core::ptr::null_mut(),
}));

/// Create a new table.
pub(crate) fn new_table() -> Result<HTable, HSearchError> {
    make_table(&mut ShellEntropy)
}

/// The C `ACTION` value.
pub(crate) fn action_of(value: c_int) -> Action {
    if value == 0 {
        Action::Find
    } else {
        Action::Enter
    }
}

/// Copy a table entry into the one C output slot.
pub(crate) fn copy_out(entry: HashEntry, out: &Output) -> *mut CEntry {
    let value = CEntry {
        key: entry.key as *mut c_char,
        data: entry.data as *mut c_void,
    };
    // SAFETY: the output slot is written for one C call at a time.
    unsafe { *out.0.get() = value };
    out.0.get()
}

/// One `hsearch`-family lookup.
pub(crate) fn search(
    table: &mut HTable,
    item: &CEntry,
    action: Action,
    out: &Output,
) -> *mut CEntry {
    match table.hsearch(item.key as usize, item.data as usize, action, &CKeys) {
        Ok(slot) => match table.entry(slot) {
            Some(entry) => copy_out(entry, out),
            None => {
                crate::ffi::errno::set(Errno(3));
                core::ptr::null_mut()
            }
        },
        Err(HSearchError::NotFound) => {
            crate::ffi::errno::set(Errno(3));
            core::ptr::null_mut()
        }
        Err(HSearchError::NoMem) => {
            crate::ffi::errno::set(Errno(12));
            core::ptr::null_mut()
        }
    }
}

/// Own a table in the caller's `hsearch_data`.
pub(crate) fn install(data: *mut HsearchData, table: HTable) {
    let boxed = Box::into_raw(Box::new(table));
    // SAFETY: the caller's struct; the box stays alive until `hdestroy_r`.
    unsafe { (*data).tab = boxed.cast::<c_void>() };
}

/// Drop the table owned by the caller's `hsearch_data`.
pub(crate) fn remove(data: *mut HsearchData) {
    if data.is_null() {
        return;
    }
    // SAFETY: the caller's struct; `tab` is a Box from `install`.
    let tab = unsafe { (*data).tab };
    if !tab.is_null() {
        drop(unsafe { Box::from_raw(tab.cast::<HTable>()) });
        // SAFETY: the caller's struct.
        unsafe { (*data).tab = core::ptr::null_mut() };
    }
}
