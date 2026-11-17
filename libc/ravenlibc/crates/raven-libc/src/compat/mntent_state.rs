// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared state and C `struct mntent` of the `mntent` exports.

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
use raven_libc_compat::mntent::{MntEnt, format, hasmntopt as logic_hasmntopt, parse_line};

/// The C `struct mntent`.
#[repr(C)]
pub struct CMntEnt {
    pub mnt_fsname: *mut c_char,
    pub mnt_dir: *mut c_char,
    pub mnt_type: *mut c_char,
    pub mnt_opts: *mut c_char,
    pub mnt_freq: c_int,
    pub mnt_passno: c_int,
}

pub(crate) static mut CURRENT: CMntEnt = CMntEnt {
    mnt_fsname: core::ptr::null_mut(),
    mnt_dir: core::ptr::null_mut(),
    mnt_type: core::ptr::null_mut(),
    mnt_opts: core::ptr::null_mut(),
    mnt_freq: 0,
    mnt_passno: 0,
};
/// The single scratch buffer shared by the line reader (`getmntent`) and the returned strings.
///
/// `getmntent` reads a line here, then [`store`] copies the parsed fields back into the same
/// buffer. The fields appear in the line in parse order, so every destination offset is at or
/// before its source offset; the overlap-safe `ptr::copy` (memmove) keeps the not-yet-copied
/// tail intact. This replaces the donor's separate line and arena buffers with one 8 KiB block.
pub(crate) static mut SCRATCH: [u8; 8192] = [0; 8192];

/// Copy `entry`'s fields into `current`, using `arena` for the NUL-terminated C strings.
///
/// `entry` may point into `arena` (the `getmntent` path); the per-field `ptr::copy` handles the
/// overlap because the parse order matches the line order.
///
/// # Safety
/// `current` must be valid for writes; `arena` must be at least the ascii length of the fields.
pub(crate) unsafe fn store_into(
    entry: &MntEnt,
    current: &mut CMntEnt,
    arena: &mut [u8; 8192],
) -> *mut CMntEnt {
    let base = arena.as_mut_ptr();
    let mut offset = 0usize;
    let mut copy = |value: &[u8], offset: &mut usize| -> *mut c_char {
        let start = *offset;
        let end = start.saturating_add(value.len()).saturating_add(1);
        if end > arena.len() {
            return core::ptr::null_mut();
        }
        // SAFETY: `start..end` is in bounds and the donor's field order guarantees the
        // destination does not run past an unread source field.
        unsafe { core::ptr::copy(value.as_ptr(), base.add(start), value.len()) };
        // SAFETY: the terminator is in bounds (`end <= arena.len()`).
        unsafe { *base.add(start + value.len()) = 0 };
        *offset = end;
        // SAFETY: `start` is in bounds.
        unsafe { base.add(start).cast::<c_char>() }
    };
    current.mnt_fsname = copy(&entry.fsname, &mut offset);
    current.mnt_dir = copy(&entry.dir, &mut offset);
    current.mnt_type = copy(&entry.type_, &mut offset);
    current.mnt_opts = copy(&entry.opts, &mut offset);
    current.mnt_freq = entry.freq;
    current.mnt_passno = entry.passno;
    current as *mut CMntEnt
}

pub(crate) unsafe fn store(entry: &MntEnt) -> *mut CMntEnt {
    // SAFETY: both statics are private and the C API is not re-entrant.
    let arena = unsafe { &mut *(&raw mut SCRATCH) };
    let current = unsafe { &mut *(&raw mut CURRENT) };
    unsafe { store_into(entry, current, arena) }
}

#[cfg(test)]
#[path = "mntent_state_ut.rs"]
mod tests;
