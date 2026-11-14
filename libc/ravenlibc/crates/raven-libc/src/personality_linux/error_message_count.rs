// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `error_message_count`, the message-counter data symbol of `error.h`.
use core::ffi::c_uint;

/// `unsigned int error_message_count`.
#[unsafe(no_mangle)]
pub static mut error_message_count: c_uint = 0;

/// Increment and return the message count.
pub(crate) fn bump() -> c_uint {
    // SAFETY: scalar read/write through `addr_of!`.
    unsafe {
        let current = core::ptr::read(core::ptr::addr_of!(error_message_count));
        let next = current.saturating_add(1);
        core::ptr::write(core::ptr::addr_of_mut!(error_message_count), next);
        next
    }
}

/// Store the message count.
pub(crate) fn set(value: c_uint) {
    // SAFETY: scalar write through `addr_of!`.
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(error_message_count), value) };
}
