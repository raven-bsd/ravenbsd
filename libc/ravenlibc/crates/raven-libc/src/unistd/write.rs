// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `write(2)`.
use core::ffi::{c_int, c_void};

use crate::personality_common::{EVENT_FDS, RealEvent, RealFdTable};

/// Write `count` bytes from `buffer`.
///
/// # Safety
/// `buffer` has `count` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize {
    // SAFETY: the caller's buffer contract; a NULL with zero is the empty slice.
    let slice = unsafe { crate::ffi::cstr::bytes_n(buffer.cast(), count) };
    if crate::personality_common::is_eventfd(fd) {
        return crate::ffi::ret::platform_ssize(EVENT_FDS.with(|state| {
            let value = eventfd_value(slice)?;
            raven_libc_personality_linux::eventfd::write(
                &RealFdTable,
                &mut RealEvent,
                state,
                fd,
                value,
            )
            .map(|()| 8)
        }));
    }
    let result = crate::ffi::platform::global().write(fd, slice);
    if fd == 9 {
        let _ = raven_libc_sys::sys::debug_write(
            alloc::format!(
                "wr9 pid={} ok={} n={} err={}\n",
                // SAFETY: the C `getpid` export takes no arguments and reads no memory.
                unsafe { crate::compat::getpid() },
                result.is_ok(),
                result.as_ref().map_or(0, |n| *n),
                result.as_ref().err().map_or(0, |e| e.0)
            )
            .as_bytes(),
        );
    }
    crate::ffi::ret::platform_ssize(result)
}

/// The 64-bit little-endian value of one `eventfd` write.
///
/// Linux requires exactly eight bytes; `eventfd_write(3)` is the same operation.
fn eventfd_value(bytes: &[u8]) -> Result<u64, raven_libc_platform::Errno> {
    let Ok(field) = <[u8; 8]>::try_from(bytes) else {
        return Err(raven_libc_platform::Errno::EINVAL);
    };
    Ok(u64::from_le_bytes(field))
}

#[cfg(test)]
#[path = "write_ut.rs"]
mod tests;
