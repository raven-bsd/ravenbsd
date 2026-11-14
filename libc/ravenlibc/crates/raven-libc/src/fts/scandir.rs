// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `scandir(3)`.
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::structs::Dirent;
use raven_libc_fts::DirEntry;

use super::fill_dirent;
use crate::platform::ShellDirSys;

/// The C selection predicate.
pub(crate) type Select = extern "C" fn(*const Dirent) -> c_int;
/// The C sort comparator.
pub(crate) type Compare = extern "C" fn(*const *const Dirent, *const *const Dirent) -> c_int;

/// Collect a directory into `namelist`.
///
/// # Safety
/// `path` is NUL terminated; `namelist` is writable; the callbacks are NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scandir(
    path: *const c_char,
    namelist: *mut *mut *mut Dirent,
    select: Option<Select>,
    compare: Option<Compare>,
) -> c_int {
    if namelist.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let mut sys = ShellDirSys;
    let mut selector = |entry: &DirEntry| -> bool { wanted(select, entry) };
    let mut comparator = |a: &DirEntry, b: &DirEntry| -> c_int { order(compare, a, b) };
    let sel: Option<&mut dyn FnMut(&DirEntry) -> bool> = if select.is_some() {
        Some(&mut selector)
    } else {
        None
    };
    let cmp: Option<&mut dyn FnMut(&DirEntry, &DirEntry) -> c_int> = if compare.is_some() {
        Some(&mut comparator)
    } else {
        None
    };
    match raven_libc_fts::scandir::scandir(&mut sys, path, sel, cmp) {
        // SAFETY: the caller's namelist is writable; each entry is copied into a fresh block.
        Ok(entries) => unsafe { publish(namelist, &entries) },
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

/// Publish the sorted entries as the C `dirent **` array.
///
/// # Safety
/// `namelist` is writable; each entry is copied into a fresh C allocation.
unsafe fn publish(namelist: *mut *mut *mut Dirent, entries: &[DirEntry]) -> c_int {
    let size = core::mem::size_of::<*mut Dirent>();
    let total = entries.len().saturating_add(1).saturating_mul(size);
    // SAFETY: the shell allocator returns `total` writable bytes or NULL.
    let array = unsafe { crate::malloc::abi_alloc(total) } as *mut *mut Dirent;
    if array.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOMEM as u16,
        )));
    }
    let mut index = 0usize;
    while index < entries.len() {
        // SAFETY: the shell allocator returns one `Dirent` or NULL.
        let slot =
            unsafe { crate::malloc::abi_alloc(core::mem::size_of::<Dirent>()) } as *mut Dirent;
        if slot.is_null() {
            // SAFETY: free the partial array and its entries.
            unsafe { free_dirents(array, index) };
            return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENOMEM as u16,
            )));
        }
        // SAFETY: `slot` is one writable `Dirent`; `fill_dirent` NUL-terminates the name.
        unsafe {
            (*slot) = Dirent::ZERO;
            if let Some(entry) = entries.get(index) {
                fill_dirent(entry, &mut *slot);
            }
            *array.add(index) = slot;
        }
        index = index.wrapping_add(1);
    }
    // SAFETY: the array has `entries.len() + 1` slots.
    unsafe {
        *array.add(entries.len()) = core::ptr::null_mut();
        *namelist = array;
    }
    match c_int::try_from(entries.len()) {
        Ok(count) => count,
        Err(_) => crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EOVERFLOW as u16,
        ))),
    }
}

/// Free `count` `dirent` allocations and the array.
///
/// # Safety
/// `array` has `count` entries allocated by `publish`.
unsafe fn free_dirents(array: *mut *mut Dirent, count: usize) {
    let mut index = 0usize;
    while index < count {
        // SAFETY: the array has `count` entries.
        let slot = unsafe { *array.add(index) };
        if !slot.is_null() {
            // SAFETY: each entry was allocated by `abi_alloc`.
            unsafe { crate::malloc::free::free(slot.cast::<c_void>()) };
        }
        index = index.wrapping_add(1);
    }
    // SAFETY: the array was allocated by `abi_alloc`.
    unsafe { crate::malloc::free::free(array.cast::<c_void>()) };
}

/// Call the C selection predicate.
fn wanted(select: Option<Select>, entry: &DirEntry) -> bool {
    let Some(func) = select else {
        return true;
    };
    let mut dirent = Dirent::ZERO;
    fill_dirent(entry, &mut dirent);
    func(&dirent) != 0
}

/// Call the C sort comparator.
fn order(compare: Option<Compare>, a: &DirEntry, b: &DirEntry) -> c_int {
    let Some(func) = compare else {
        return 0;
    };
    let mut left = Dirent::ZERO;
    let mut right = Dirent::ZERO;
    fill_dirent(a, &mut left);
    fill_dirent(b, &mut right);
    let left_ref: *const Dirent = &left;
    let right_ref: *const Dirent = &right;
    func(&left_ref, &right_ref)
}
