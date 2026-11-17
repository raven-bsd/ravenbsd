// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `if_nameindex` from `<net/if.h>`.

use core::ffi::c_char;

use raven_libc_abi::errno::Errno;

use super::if_names::CIfNameIndex;

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
    // SAFETY: `names_start` is an offset inside the same `total`-byte block allocated above.
    let names_start =
        unsafe { block.add(core::mem::size_of::<CIfNameIndex>().saturating_mul(slots)) };
    let mut slot = 0usize;
    let mut at = 0usize;
    for (index, name) in &entries {
        // SAFETY: `at` stays inside the same block; it never exceeds the allocated total.
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
