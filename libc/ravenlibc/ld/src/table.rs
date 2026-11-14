// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `RtldTable` contract with the C64 forwarding stubs.

/// The C ABI table installed into `libc.so.1`.
#[repr(C)]
pub struct RtldTable {
    /// Contract version, always 1.
    pub version: u32,
    /// `size_of::<RtldTable>()`.
    pub size: u32,
    /// `dlopen`.
    pub dlopen: extern "C" fn(*const u8, i32) -> *mut u8,
    /// `dlclose`.
    pub dlclose: extern "C" fn(*mut u8) -> i32,
    /// `dlsym`; the last argument is the caller's return address.
    pub dlsym: extern "C" fn(*mut u8, *const u8, usize) -> *mut u8,
    /// `dlvsym`.
    pub dlvsym: extern "C" fn(*mut u8, *const u8, *const u8, usize) -> *mut u8,
    /// `dlerror`.
    pub dlerror: extern "C" fn() -> *const u8,
    /// `dladdr`.
    pub dladdr: extern "C" fn(usize, *mut u8) -> i32,
    /// `dlinfo`.
    pub dlinfo: extern "C" fn(*mut u8, i32, *mut u8) -> i32,
    /// `dl_iterate_phdr`.
    pub dl_iterate_phdr:
        extern "C" fn(Option<extern "C" fn(*mut u8, usize, *mut u8) -> i32>, *mut u8) -> i32,
}

impl RtldTable {
    /// The contract version.
    pub const VERSION: u32 = 1;
    /// The table size.
    pub const SIZE: u32 = core::mem::size_of::<Self>() as u32;
}

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;
