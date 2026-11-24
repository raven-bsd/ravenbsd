// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! K571/S188 experiment: the exit-time order. A `thread_local!` is initialized, an `atexit`
//! handler reads it, and main calls `exit(0)`. glibc runs the atexit/`__cxa_finalize` handlers
//! while TLS is addressable and only then `__call_tls_dtors`; if our libc destroys TLS first, the
//! handler panics with the shell's AccessError.

use std::cell::RefCell;

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        println!("ATEXIT-TLS guard drop");
    }
}

thread_local! {
    static VALUE: RefCell<Option<Guard>> = RefCell::new(Some(Guard));
    static MASK: RefCell<u32> = const { RefCell::new(7) };
}

extern "C" fn on_exit() {
    MASK.with(|value| println!("ATEXIT-TLS value={}", *value.borrow()));
}

unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> i32;
}

fn main() {
    MASK.with(|value| println!("MAIN-TLS value={}", *value.borrow()));
    // SAFETY: the callback is a plain C function that only reads a live thread-local.
    let registered = unsafe { atexit(on_exit) };
    println!("MAIN-TLS atexit={registered}");
    std::process::exit(0);
}
