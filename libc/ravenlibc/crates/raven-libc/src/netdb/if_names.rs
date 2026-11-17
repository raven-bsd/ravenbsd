// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `if_*` family from `<net/if.h>` over C63's interface list.

use core::ffi::{c_char, c_uint, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_resolv::IfNames;

/// The C `struct if_nameindex`.
#[repr(C)]
pub struct CIfNameIndex {
    /// Interface index.
    pub if_index: c_uint,
    /// Interface name.
    pub if_name: *mut c_char,
}

/// `if_nametoindex`.
///
/// # Safety
/// `name` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn if_nametoindex(name: *const c_char) -> c_uint {
    if name.is_null() {
        crate::ffi::errno::set(Errno(14));
        return 0;
    }
    // SAFETY: the caller's C string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(name) };
    let Ok(text) = core::str::from_utf8(bytes) else {
        crate::ffi::errno::set(Errno(19));
        return 0;
    };
    let index = crate::socket::with_net(|net| net.ifnames().name_to_index(text));
    match index {
        Some(index) => index,
        None => {
            crate::ffi::errno::set(Errno(19));
            0
        }
    }
}

/// `if_indextoname`.
///
/// # Safety
/// `buffer` has at least `IF_NAMESIZE` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn if_indextoname(index: c_uint, buffer: *mut c_char) -> *mut c_char {
    if buffer.is_null() {
        crate::ffi::errno::set(Errno(14));
        return core::ptr::null_mut();
    }
    let name = crate::socket::with_net(|net| net.ifnames().index_to_name(index));
    match name {
        Some(name) => {
            // SAFETY: the caller's `IF_NAMESIZE` buffer.
            unsafe { crate::ffi::out::write_cstr(buffer, name.as_bytes(), 16) };
            buffer
        }
        None => crate::ffi::ret::null(Errno(6)),
    }
}

/// `if_nameindex`.
#[unsafe(no_mangle)]
pub extern "C" fn if_nameindex() -> *mut CIfNameIndex {
    let entries = match crate::socket::with_net(|net| net.iface_entries()) {
        Ok(entries) => entries,
        Err(error) => {
            crate::ffi::errno::set(error);
            return core::ptr::null_mut();
        }
    };
    let slots = entries.len().saturating_add(1);
    let mut total = core::mem::size_of::<CIfNameIndex>().saturating_mul(slots);
    for (_, name) in &entries {
        total = total.saturating_add(name.len()).saturating_add(1);
    }
    // SAFETY: the shell allocator; the caller frees with `if_freenameindex`.
    let block = unsafe { crate::malloc::abi_alloc(total) } as *mut u8;
    if block.is_null() {
        crate::ffi::errno::set(Errno(12));
        return core::ptr::null_mut();
    }
    let array = block.cast::<CIfNameIndex>();
    let names_start =
        unsafe { block.add(core::mem::size_of::<CIfNameIndex>().saturating_mul(slots)) };
    let mut slot = 0usize;
    let mut at = 0usize;
    for (index, name) in &entries {
        let destination = unsafe { names_start.add(at) };
        // SAFETY: the block has room for the name, its NUL and the array slot.
        unsafe {
            core::ptr::copy_nonoverlapping(name.as_ptr(), destination, name.len());
            *destination.add(name.len()) = 0;
            *array.add(slot) = CIfNameIndex {
                if_index: *index,
                if_name: destination.cast::<c_char>(),
            };
        }
        at = at.saturating_add(name.len()).saturating_add(1);
        slot = slot.saturating_add(1);
    }
    // SAFETY: the terminator slot is inside the array allocation.
    unsafe {
        *array.add(slot) = CIfNameIndex {
            if_index: 0,
            if_name: core::ptr::null_mut(),
        };
    }
    array
}

/// `if_freenameindex`.
///
/// # Safety
/// `pointer` is NULL or the result of `if_nameindex`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn if_freenameindex(pointer: *mut CIfNameIndex) {
    if pointer.is_null() {
        return;
    }
    // SAFETY: the pointer is the one block from `if_nameindex`.
    unsafe { crate::malloc::free::free(pointer.cast::<c_void>()) };
}
