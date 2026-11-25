// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! S194 diagnostic: the Rust `libc` crate's RavenBSD `struct sigaction` is 24 bytes
//! (`sighandler_t`, `c_int` flags, `sigset_t` = `c_ulong`), while `ravenlibc`'s external
//! `sigaction(3)` writes the 152-byte C struct (`sa_handler`, 128-byte `sa_mask`, flags,
//! restorer). A query call must not write past the caller's object. The shell calls
//! `sigaction(SIGTTOU, SIG_IGN)` through `nix` with a 24-byte `oldact`.

use std::io::Error;

/// `struct sigaction` as the vendored Rust `libc` crate defines it for `target_os = "ravenbsd"`.
#[repr(C)]
#[derive(Clone, Copy)]
struct SmallAction {
    handler: usize,
    flags: i32,
    _pad: i32,
    mask: u64,
}

/// The C `struct sigaction` from the generated RavenBSD headers: 152 bytes.
#[repr(C)]
struct FullAction {
    handler: usize,
    mask: [u8; 128],
    flags: i32,
    _pad: i32,
    restorer: usize,
}

const SIGTTOU: i32 = 22;
const SIG_IGN: usize = 1;
const SMALL_SIZE: usize = core::mem::size_of::<SmallAction>();
const FULL_SIZE: usize = core::mem::size_of::<FullAction>();

unsafe extern "C" {
    fn sigaction(sig: i32, new: *const SmallAction, old: *mut SmallAction) -> i32;
}

fn main() {
    println!("SIGACTION-ABI: small={SMALL_SIZE} full={FULL_SIZE}");
    if SMALL_SIZE != 24 || FULL_SIZE != 152 {
        println!("SIGACTION-ABI: FAIL layout");
        std::process::exit(1);
    }

    // Query only: `new` is NULL, so libc must report the previous action into `old`. A 152-byte
    // write from a 24-byte caller object is an overflow and must not happen.
    let mut buffer = [0xAAu8; 256];
    let small = SmallAction {
        handler: SIG_IGN,
        flags: 0,
        _pad: 0,
        mask: 0,
    };
    // SAFETY: the buffer is 256 bytes and the small action fits at offset 0.
    unsafe {
        core::ptr::write(buffer.as_mut_ptr() as *mut SmallAction, small);
    }
    let query = unsafe {
        // SAFETY: `old` points at 24 writable bytes at the start of a 256-byte buffer.
        sigaction(
            SIGTTOU,
            core::ptr::null(),
            buffer.as_mut_ptr() as *mut SmallAction,
        )
    };
    let query_errno = if query < 0 {
        Error::last_os_error().raw_os_error().unwrap_or(0)
    } else {
        0
    };
    let overflowed = buffer[SMALL_SIZE..FULL_SIZE]
        .iter()
        .filter(|byte| **byte != 0xAA)
        .count();
    println!(
        "SIGACTION-ABI: query rc={query} errno={query_errno} overflow-bytes={overflowed}"
    );

    // Install with the small layout and no old action. The overread of flags/mask must not make
    // the kernel reject the request, and the call must not write past the caller's object.
    let mut install_buffer = [0xAAu8; 256];
    unsafe {
        core::ptr::write(install_buffer.as_mut_ptr() as *mut SmallAction, small);
    }
    let install = unsafe {
        // SAFETY: the action is the 24-byte layout at offset 0 of a 256-byte buffer, and `old`
        // is NULL.
        sigaction(
            SIGTTOU,
            install_buffer.as_ptr() as *const SmallAction,
            core::ptr::null_mut(),
        )
    };
    let install_errno = if install < 0 {
        Error::last_os_error().raw_os_error().unwrap_or(0)
    } else {
        0
    };
    let install_overrun = install_buffer[SMALL_SIZE..FULL_SIZE]
        .iter()
        .filter(|byte| **byte != 0xAA)
        .count();
    println!(
        "SIGACTION-ABI: install rc={install} errno={install_errno} overflow-bytes={install_overrun}"
    );

    // The generated C layout must be accepted with the same request.
    let mut full = FullAction {
        handler: SIG_IGN,
        mask: [0; 128],
        flags: 0,
        _pad: 0,
        restorer: 0,
    };
    let full_rc = unsafe {
        // SAFETY: `full` is the 152-byte C layout and `old` is NULL.
        sigaction(
            SIGTTOU,
            &full as *const FullAction as *const SmallAction,
            core::ptr::null_mut(),
        )
    };
    let full_errno = if full_rc < 0 {
        Error::last_os_error().raw_os_error().unwrap_or(0)
    } else {
        0
    };
    println!("SIGACTION-ABI: full rc={full_rc} errno={full_errno}");

    if overflowed != 0 || install_overrun != 0 || query != 0 || install != 0 || full_rc != 0 {
        println!("SIGACTION-ABI: FAIL overflow or rejected action");
        std::process::exit(1);
    }
    println!("SIGACTION-ABI: pass");
    std::process::exit(0);
}
