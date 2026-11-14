// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The map tables of the name-service family (C64 4.7): `EAI_*` codes and their texts, the
//! legacy `h_errno` values, the `herror` line and the `inet_*` wrappers over C51's resolver.
//! The calls that need a `Net`/`Env` seam (sockets, `getaddrinfo`, the host/service file
//! iteration) are deferred with C63; every pure codec export goes through this module and is
//! covered by the host tests and `hostrun/h8_net.rs`.

use core::ffi::c_char;

use raven_libc_abi::errno::{EAFNOSUPPORT, ENOSPC, Errno};
use raven_libc_resolv::Eai;

/// The C `EAI_*` code of one crate error.
pub(crate) fn eai_code(value: Eai) -> i32 {
    value.code()
}

/// The `gai_strerror` text (musl table, `"Unknown error"` otherwise).
pub(crate) fn gai_text(code: i32) -> &'static str {
    raven_libc_resolv::gai_strerror(code)
}

/// The `hstrerror` text.
pub(crate) fn hstrerror_text(code: i32) -> &'static str {
    raven_libc_resolv::compat::hstrerror(code)
}

/// The `h_errno` value a `getaddrinfo`/`getnameinfo` failure maps to.
pub(crate) fn h_errno_for(value: Eai) -> i32 {
    value.to_herror()
}

/// The `herror` line (text plus newline) the shell writes to descriptor 2.
pub(crate) fn herror_line(message: &[u8], code: i32) -> alloc::vec::Vec<u8> {
    let text = core::str::from_utf8(message).ok();
    let mut line = raven_libc_resolv::compat::herror(text, code).into_bytes();
    line.push(b'\n');
    line
}

/// A static text as a C string pointer, copied into the scratch area with its NUL.
///
/// Rust string literals are not NUL-terminated; the resolver's `&'static str` return values
/// therefore need the one copy. The pointer stays valid until the next scratch-area call, the
/// same contract as `inet_ntoa`.
pub(crate) fn text_pointer(text: &'static str) -> *const c_char {
    crate::ffi::tcbbuf::with_area(|area| {
        crate::ffi::tcbbuf::copy_text(&mut area.name.bytes, text.as_bytes())
            .map_or(core::ptr::null(), |pointer| pointer.cast_const())
    })
}

/// `inet_pton` with the C return convention (`1`, `0`, `-1` plus `errno`).
pub(crate) fn pton(af: i32, source: &[u8], out: &mut [u8]) -> i32 {
    let result = raven_libc_resolv::inet_pton(af, source, out);
    if result < 0 {
        crate::ffi::errno::set(Errno(EAFNOSUPPORT as u16));
    }
    result
}

/// `inet_ntop` with the C pointer result and `errno` on failure.
pub(crate) fn ntop(af: i32, source: &[u8], out: &mut [u8]) -> *mut c_char {
    match raven_libc_resolv::inet_ntop(af, source, out) {
        Ok(_) => out.as_mut_ptr().cast::<c_char>(),
        Err(error) => {
            let value = if error == raven_libc_abi::errno::ENOSPC {
                ENOSPC
            } else if error == raven_libc_abi::errno::EAFNOSUPPORT {
                EAFNOSUPPORT
            } else {
                error
            };
            crate::ffi::errno::set(Errno(value as u16));
            core::ptr::null_mut()
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "map_net_ut.rs"]
mod tests;
