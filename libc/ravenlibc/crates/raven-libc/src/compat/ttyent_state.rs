// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared state of the `ttyent` exports (the donor's static `ttyent` and stream).

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

use raven_libc_compat::ReadFile;
use raven_libc_compat::ttyent::{TtyDb, TtyEnt};

/// The C `struct ttyent`.
#[repr(C)]
pub struct CTtyEnt {
    pub ty_name: *mut c_char,
    pub ty_getty: *mut c_char,
    pub ty_type: *mut c_char,
    pub ty_status: c_int,
    pub ty_window: *mut c_char,
    pub ty_comment: *mut c_char,
    pub ty_group: *mut c_char,
}

pub(crate) static mut TTYDB: Option<TtyDb> = None;
static mut ARENA: [u8; 8192] = [0; 8192];
static mut CURRENT: CTtyEnt = CTtyEnt {
    ty_name: core::ptr::null_mut(),
    ty_getty: core::ptr::null_mut(),
    ty_type: core::ptr::null_mut(),
    ty_status: 0,
    ty_window: core::ptr::null_mut(),
    ty_comment: core::ptr::null_mut(),
    ty_group: core::ptr::null_mut(),
};

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
        // SAFETY: `name` is NUL-terminated and `open` returns a descriptor or -1.
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

pub(crate) unsafe fn load() -> bool {
    // SAFETY: the C API is not re-entrant; the static is the donor's global.
    let slot = &raw mut TTYDB;
    match TtyDb::open(&mut RealFiles, b"/etc/ttys") {
        Ok(database) => {
            unsafe { *slot = Some(database) };
            true
        }
        Err(_) => false,
    }
}

pub(crate) unsafe fn fill(entry: &TtyEnt) -> *mut CTtyEnt {
    // SAFETY: ARENA and CURRENT are private statics; the C API is not re-entrant.
    let arena = unsafe { &raw mut ARENA };
    let current = unsafe { &raw mut CURRENT };
    let arena = unsafe { &mut *arena };
    let mut offset = 0usize;
    let mut copy = |value: &[u8], arena: &mut [u8; 8192], offset: &mut usize| -> *mut c_char {
        let start = *offset;
        let end = start.saturating_add(value.len()).saturating_add(1);
        if end > arena.len() {
            return core::ptr::null_mut();
        }
        arena[start..start + value.len()].copy_from_slice(value);
        arena[start + value.len()] = 0;
        *offset = end;
        arena[start..].as_mut_ptr().cast::<c_char>()
    };
    // SAFETY: `current` points at the private static CTtyEnt.
    let pointer = unsafe { &mut *current };
    pointer.ty_name = copy(&entry.name, arena, &mut offset);
    pointer.ty_getty = copy(&entry.getty, arena, &mut offset);
    pointer.ty_type = copy(&entry.type_, arena, &mut offset);
    pointer.ty_status = entry.status as c_int;
    pointer.ty_window = copy(&entry.window, arena, &mut offset);
    pointer.ty_comment = copy(&entry.comment, arena, &mut offset);
    pointer.ty_group = copy(&entry.group, arena, &mut offset);
    current.cast()
}
