// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigaction(3)`.
use core::ffi::c_int;

use raven_libc_signal::Disposition;

use super::DISP;

/// The C `struct sigaction` (152 bytes; C36 layout).
#[repr(C)]
pub(crate) struct SigAction {
    pub(crate) handler: usize,
    pub(crate) mask: [u64; 16],
    pub(crate) flags: c_int,
    pub(crate) pad: c_int,
    pub(crate) restorer: usize,
}

/// Install or query a disposition.
///
/// # Safety
/// `new`/`old` are NULL or point to `struct sigaction` objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(
    sig: c_int,
    new: *const SigAction,
    old: *mut SigAction,
) -> c_int {
    let request = if new.is_null() {
        None
    } else {
        // SAFETY: the caller supplies a readable sigaction.
        let action = unsafe { &*new };
        Some(Disposition {
            handler: action.handler,
            mask: action.mask[0],
            flags: action.flags as u32,
        })
    };
    let mut previous = Disposition::DEFAULT;
    match raven_libc_signal::dispositions::sigaction(
        &DISP,
        sig,
        request.as_ref(),
        if old.is_null() {
            None
        } else {
            Some(&mut previous)
        },
    ) {
        Ok(()) => {
            if !old.is_null() {
                // SAFETY: the caller supplies a writable sigaction.
                unsafe {
                    *old = SigAction {
                        handler: previous.handler,
                        mask: [0; 16],
                        flags: previous.flags as c_int,
                        pad: 0,
                        restorer: 0,
                    };
                    (*old).mask[0] = previous.mask;
                }
            }
            0
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
