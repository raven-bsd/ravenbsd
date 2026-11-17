// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Regression tests for the single scratch buffer shared by the `getmntent` line reader and
//! the returned C strings.

use super::*;
use core::ffi::CStr;

fn entry() -> MntEnt {
    MntEnt {
        fsname: b"/dev/ada0p2".to_vec(),
        dir: b"/".to_vec(),
        type_: b"ufs".to_vec(),
        opts: b"rw,noatime".to_vec(),
        freq: 1,
        passno: 2,
    }
}

fn empty_current() -> CMntEnt {
    CMntEnt {
        mnt_fsname: core::ptr::null_mut(),
        mnt_dir: core::ptr::null_mut(),
        mnt_type: core::ptr::null_mut(),
        mnt_opts: core::ptr::null_mut(),
        mnt_freq: 0,
        mnt_passno: 0,
    }
}

unsafe fn text(pointer: *mut c_char) -> Vec<u8> {
    // SAFETY: `store_into` writes a NUL-terminated string into `arena` for every field.
    unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec()
}

#[test]
fn the_shared_buffer_carries_every_field() {
    let mut arena = [0u8; 8192];
    let mut current = empty_current();
    let parsed = entry();
    // SAFETY: `arena` and `current` are local and the entry is independent of them.
    let returned = unsafe { store_into(&parsed, &mut current, &mut arena) };
    assert_eq!(returned, &raw mut current);
    // SAFETY: every pointer was produced by `store_into` above.
    unsafe {
        assert_eq!(text(current.mnt_fsname), b"/dev/ada0p2");
        assert_eq!(text(current.mnt_dir), b"/");
        assert_eq!(text(current.mnt_type), b"ufs");
        assert_eq!(text(current.mnt_opts), b"rw,noatime");
    }
    assert_eq!(current.mnt_freq, 1);
    assert_eq!(current.mnt_passno, 2);
}

#[test]
fn a_second_store_reuses_the_arena_from_offset_zero() {
    let mut arena = [0u8; 8192];
    let mut current = empty_current();
    // SAFETY: local buffers only.
    unsafe {
        store_into(&entry(), &mut current, &mut arena);
        let second = MntEnt {
            fsname: b"proc".to_vec(),
            dir: b"/proc".to_vec(),
            type_: b"procfs".to_vec(),
            opts: b"ro".to_vec(),
            freq: 0,
            passno: 0,
        };
        store_into(&second, &mut current, &mut arena);
        assert_eq!(text(current.mnt_fsname), b"proc");
        assert_eq!(text(current.mnt_dir), b"/proc");
        assert_eq!(text(current.mnt_type), b"procfs");
        assert_eq!(text(current.mnt_opts), b"ro");
    }
}

#[test]
fn the_old_strings_stay_valid_until_the_next_call() {
    let mut arena = [0u8; 8192];
    let mut current = empty_current();
    // SAFETY: local buffers only.
    unsafe {
        store_into(&entry(), &mut current, &mut arena);
        let first = current.mnt_dir;
        assert_eq!(text(first), b"/");
        // A different store must not leave the previous field content at its old address.
        let second = MntEnt {
            fsname: b"devfs".to_vec(),
            dir: b"/dev".to_vec(),
            type_: b"devfs".to_vec(),
            opts: b"rw".to_vec(),
            freq: 0,
            passno: 0,
        };
        store_into(&second, &mut current, &mut arena);
        assert_eq!(text(current.mnt_dir), b"/dev");
    }
}
