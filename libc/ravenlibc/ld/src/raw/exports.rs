// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The 17 exported loader symbols and the minimal target entry glue.
//!
//! The real forwarding bodies are C64's; this file only keeps the ABI (and the `libc-link`
//! export set) valid for the target build.

use crate::raw::sys;

/// `_rtld_start` target helper; the assembly stub jumps here.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_main() -> ! {
    sys::debug_write(b"ld-elf.so.1: no direct target start yet\n");
    sys::exit(127)
}

/// `dlopen`.
#[unsafe(no_mangle)]
pub extern "C" fn dlopen(_name: *const u8, _flags: i32) -> *mut u8 {
    core::ptr::null_mut()
}

/// `dlclose`.
#[unsafe(no_mangle)]
pub extern "C" fn dlclose(_handle: *mut u8) -> i32 {
    -1
}

/// `dlsym`.
#[unsafe(no_mangle)]
pub extern "C" fn dlsym(_handle: *mut u8, _name: *const u8, _caller: usize) -> *mut u8 {
    core::ptr::null_mut()
}

/// `dlvsym`.
#[unsafe(no_mangle)]
pub extern "C" fn dlvsym(
    _handle: *mut u8,
    _name: *const u8,
    _version: *const u8,
    _caller: usize,
) -> *mut u8 {
    core::ptr::null_mut()
}

/// `dlerror`.
#[unsafe(no_mangle)]
pub extern "C" fn dlerror() -> *const u8 {
    core::ptr::null()
}

/// `dladdr`.
#[unsafe(no_mangle)]
pub extern "C" fn dladdr(_address: usize, _info: *mut u8) -> i32 {
    0
}

/// `dlinfo`.
#[unsafe(no_mangle)]
pub extern "C" fn dlinfo(_handle: *mut u8, _request: i32, _argument: *mut u8) -> i32 {
    -1
}

/// `dl_iterate_phdr`.
#[unsafe(no_mangle)]
pub extern "C" fn dl_iterate_phdr(
    _callback: Option<unsafe extern "C" fn(*mut u8, usize, *mut u8) -> i32>,
    _data: *mut u8,
) -> i32 {
    0
}

/// `_rtld_error_str`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_error_str() -> *const u8 {
    core::ptr::null()
}

/// `_rtld_allocate_tls`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_allocate_tls(old: *mut u8, _new_size: usize) -> *mut u8 {
    old
}

/// `_rtld_free_tls`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_free_tls(ptr: *mut u8, _size: usize) -> i32 {
    let _ = ptr;
    0
}

/// `_rtld_get_stack_prot`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_get_stack_prot() -> usize {
    0
}

/// `_rtld_set_dlerror_slot`.
#[unsafe(no_mangle)]
pub extern "C" fn _rtld_set_dlerror_slot(_slot: *mut u8) {}

/// `r_debug_state`.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn r_debug_state() {}

/// `_r_debug_postinit`.
#[unsafe(no_mangle)]
pub extern "C" fn _r_debug_postinit() {}

/// `_rtld_version`.
#[unsafe(no_mangle)]
pub static _rtld_version: u32 = 1;

/// `__tls_get_addr`.
#[unsafe(no_mangle)]
pub extern "C" fn __tls_get_addr(_index: *mut u8) -> *mut u8 {
    core::ptr::null_mut()
}

/// The Raven private install hook (a libc export, not a loader export).
pub const INSTALL: &[u8] = b"__rl_dl_install";
