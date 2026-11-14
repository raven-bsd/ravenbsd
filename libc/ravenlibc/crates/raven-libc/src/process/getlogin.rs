// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getlogin(3)`.
use core::ffi::c_char;

/// The login name, or NULL/`ENXIO` without a session (PG-56).
#[unsafe(no_mangle)]
pub extern "C" fn getlogin() -> *mut c_char {
    match crate::ffi::map_proc::login(&crate::ffi::procbackend::backend()) {
        Ok(name) => crate::ffi::tcbbuf::with_area(|area| {
            crate::ffi::tcbbuf::copy_text(&mut area.login.bytes, &name)
                .unwrap_or(core::ptr::null_mut())
        }),
        Err(_) => {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENXIO as u16,
            ));
            core::ptr::null_mut()
        }
    }
}
