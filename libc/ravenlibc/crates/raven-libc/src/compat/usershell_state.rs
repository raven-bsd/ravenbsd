// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared state of the `getusershell` exports.

#![allow(
    missing_docs,
    dead_code,
    unused_imports,
    unused_unsafe,
    unused_variables,
    unused_mut,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::missing_safety_doc,
    clippy::deref_addrof,
    clippy::unreachable,
    clippy::byte_char_slices,
    clippy::collapsible_if,
    clippy::manual_c_str_literals,
    clippy::needless_return
)]

use core::ffi::c_char;

use raven_libc_compat::ReadFile;
use raven_libc_compat::usershell::Shells;

pub(crate) static mut SHELLS: Option<Shells> = None;
pub(crate) static mut SHELL_BUF: [u8; 4096] = [0; 4096];

pub(crate) struct RealFiles;

impl ReadFile for RealFiles {
    fn read_all(
        &mut self,
        path: &[u8],
        out: &mut alloc::vec::Vec<u8>,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        if path.len() >= super::MAX_NAME {
            return Err(raven_libc_abi::errno::Errno(super::ENAMETOOLONG as u16));
        }
        let mut name = [0i8; 4096];
        for (slot, byte) in name.iter_mut().zip(path.iter()) {
            *slot = *byte as i8;
        }
        name[path.len()] = 0;
        // SAFETY: `name` is NUL-terminated.
        let descriptor = unsafe { super::open(name.as_ptr(), 0) };
        if descriptor < 0 {
            return Err(raven_libc_abi::errno::Errno(super::ENOENT as u16));
        }
        out.clear();
        let mut buffer = [0u8; 8192];
        loop {
            // SAFETY: the descriptor is open and the buffer writable.
            let count =
                unsafe { super::read(descriptor, buffer.as_mut_ptr().cast(), buffer.len()) };
            if count < 0 {
                // SAFETY: the descriptor is open.
                unsafe { super::close(descriptor) };
                return Err(raven_libc_abi::errno::Errno(super::EINVAL as u16));
            }
            if count == 0 {
                break;
            }
            out.extend_from_slice(&buffer[..usize::try_from(count).unwrap_or(0)]);
        }
        // SAFETY: the descriptor is open.
        unsafe { super::close(descriptor) };
        Ok(())
    }
}

pub(crate) unsafe fn init() {
    // SAFETY: the static is private; the C API is not re-entrant.
    unsafe { *&raw mut SHELLS = Some(Shells::load(&mut RealFiles)) };
}
