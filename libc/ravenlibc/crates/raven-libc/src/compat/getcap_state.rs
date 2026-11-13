// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared state and file seam of the `getcap` exports (the donor's globals).

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

use core::ffi::{c_char, c_int};

use alloc::vec::Vec;
use raven_libc_compat::ReadFile;
use raven_libc_compat::getcap::{CapDb, CgetError};

static mut CAPDB: Option<CapDb> = None;

/// The whole-file seam over the C `open`/`read`/`close`.
pub(crate) struct RealFiles;

impl ReadFile for RealFiles {
    fn read_all(
        &mut self,
        path: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        if path.len() >= super::MAX_NAME {
            return Err(raven_libc_abi::errno::Errno(super::ENAMETOOLONG as u16));
        }
        let mut name = [0i8; 4096];
        for (slot, byte) in name.iter_mut().zip(path.iter()) {
            *slot = *byte as i8;
        }
        name[path.len()] = 0;
        // SAFETY: `name` is a NUL-terminated path and `open` returns a descriptor or -1.
        let descriptor = unsafe { super::open(name.as_ptr(), 0) };
        if descriptor < 0 {
            return Err(raven_libc_abi::errno::Errno(super::ENOENT as u16));
        }
        out.clear();
        let mut buffer = [0u8; 8192];
        loop {
            // SAFETY: `descriptor` is open and `buffer` is writable.
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
            if out.len() > raven_libc_compat::seams::READ_FILE_CAP {
                // SAFETY: the descriptor is open.
                unsafe { super::close(descriptor) };
                return Err(raven_libc_abi::errno::Errno(super::ERANGE as u16));
            }
        }
        // SAFETY: the descriptor is open.
        unsafe { super::close(descriptor) };
        Ok(())
    }
}

pub(crate) fn error_code(error: CgetError) -> c_int {
    match error {
        CgetError::NotFound => -1,
        CgetError::Loop => -3,
        CgetError::Nomem | CgetError::Errno(_) => -2,
        CgetError::Syntax => -4,
    }
}

pub(crate) unsafe fn db<'a>() -> &'a mut CapDb {
    // SAFETY: the C API is not re-entrant; the static is the donor's own global state.
    let slot = &raw mut CAPDB;
    if unsafe { (*slot).is_none() } {
        unsafe { *slot = Some(CapDb::new()) };
    }
    match unsafe { (*slot).as_mut() } {
        Some(database) => database,
        None => unreachable!(),
    }
}

pub(crate) unsafe fn files<'a>(db_array: *mut *mut c_char) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    if db_array.is_null() {
        return out;
    }
    let mut index = 0usize;
    loop {
        // SAFETY: the C contract is a NULL-terminated array of C strings.
        let entry = unsafe { *db_array.add(index) };
        if entry.is_null() {
            break;
        }
        // SAFETY: each entry is NUL-terminated.
        out.push(unsafe { super::c_bytes(entry, super::MAX_NAME) });
        index = index.wrapping_add(1);
    }
    out
}
