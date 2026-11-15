// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Minimal Rust std link probe for C64d. It is linked, not run: the body forces
//! the linker to resolve `readv`, `fcntl`, `realpath` and the unwind entry point
//! against the ravenlibc archive, while `std::hint::black_box` keeps `std` in the
//! link. `#![no_main]` makes the probe independent of the std `lang_start` path.

#![no_main]

use core::ffi::{c_char, c_int, c_void};

#[repr(C)]
struct Iovec {
    base: *mut c_void,
    length: usize,
}

unsafe extern "C" {
    fn readv(fd: c_int, iov: *const Iovec, count: c_int) -> isize;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
    fn _Unwind_Backtrace(
        trace: extern "C" fn(*mut c_void, *mut c_void) -> c_int,
        argument: *mut c_void,
    ) -> c_int;
}

extern "C" fn trace(_context: *mut c_void, _argument: *mut c_void) -> c_int {
    5 // _URC_END_OF_STACK
}

/// The crt1 `_start` calls this; the body is never part of the link gate's runtime.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    let mut byte = 0u8;
    let iov = Iovec {
        base: (&mut byte as *mut u8).cast(),
        length: 1,
    };
    let mut resolved = [0 as c_char; 4096];
    let path = b"/\0".as_ptr().cast();
    let _ = std::hint::black_box(0usize);
    // SAFETY: none of the calls runs; the declarations match the C prototypes and the
    // pointers are valid for the duration of the call.
    unsafe {
        let _ = readv(-1, &iov, 1);
        let _ = fcntl(-1, 3); // F_GETFL, no third argument
        let _ = fcntl(-1, 2, 1); // F_SETFD, int third argument
        let _ = realpath(path, resolved.as_mut_ptr());
        let _ = _Unwind_Backtrace(trace, core::ptr::null_mut());
    }
    0
}
