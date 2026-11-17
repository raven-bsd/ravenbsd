// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ttyname`.

use super::*;

/// Return the terminal name for a descriptor.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ttyname(fd: c_int) -> *mut c_char {
    static mut BUFFER: [u8; 256] = [0; 256];
    match raven_libc_platform::compat::ttyname(global(), fd) {
        Ok(name) => {
            // `addr_of_mut!` avoids forming a reference to the mutable static.
            let buffer = core::ptr::addr_of_mut!(BUFFER).cast::<u8>();
            let count = name.len().min(255);
            // SAFETY: `count <= 255` fits the static buffer.
            unsafe {
                core::ptr::copy_nonoverlapping(name.as_ptr(), buffer, count);
                *buffer.add(count) = 0;
            }
            buffer.cast()
        }
        Err(error) => ret::null(map_error(error)),
    }
}
