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

pub(crate) static mut ARENA: [u8; 8192] = [0; 8192];
pub(crate) static mut CURRENT: CMntEnt = CMntEnt {
    mnt_fsname: core::ptr::null_mut(),
    mnt_dir: core::ptr::null_mut(),
    mnt_type: core::ptr::null_mut(),
    mnt_opts: core::ptr::null_mut(),
    mnt_freq: 0,
    mnt_passno: 0,
};
pub(crate) static mut LINE: [u8; 8192] = [0; 8192];

pub(crate) unsafe fn store(entry: &MntEnt) -> *mut CMntEnt {
    // SAFETY: both statics are private and the C API is not re-entrant.
    let arena = unsafe { &mut *(&raw mut ARENA) };
    let current = unsafe { &mut *(&raw mut CURRENT) };
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
    current.mnt_fsname = copy(&entry.fsname, arena, &mut offset);
    current.mnt_dir = copy(&entry.dir, arena, &mut offset);
    current.mnt_type = copy(&entry.type_, arena, &mut offset);
    current.mnt_opts = copy(&entry.opts, arena, &mut offset);
    current.mnt_freq = entry.freq;
    current.mnt_passno = entry.passno;
    current as *mut CMntEnt
}
