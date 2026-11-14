// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// START-FINDING support for the size probes.
//
// `C61` ships `__raven_libc_start`, which calls `__cxa_atexit`,
// `__raven_init_main_thread`, the four program-name aliases and `__stack_chk_guard`.
// The owning tickets (`C41` exit registry, `C64` pthread init, shell data symbols) have
// not shipped those definitions on this branch, so `libc.a` alone cannot link a program.
// This test-owned object supplies the same definitions `C61`'s H4 fixture used; it is
// compiled once per probe directory and is NOT part of `libc.a`. When the owning symbols
// land, delete this file and the `start_support.o` argument.
#![no_std]
use core::ffi::c_void;

#[unsafe(no_mangle)]
pub static mut __progname: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut __progname_full: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut program_invocation_name: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut program_invocation_short_name: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut __stack_chk_guard: usize = 0;

#[unsafe(no_mangle)]
pub extern "C" fn __cxa_atexit(
    _func: Option<extern "C" fn(*mut c_void)>,
    _arg: *mut c_void,
    _dso: *mut c_void,
) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __raven_init_main_thread(_auxv: *const usize, _canary: usize) {}
