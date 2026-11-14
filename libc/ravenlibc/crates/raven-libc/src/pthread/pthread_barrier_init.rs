// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_barrier_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::barrier as b;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_barrier_init(
    barrier: *mut b::Barrier,
    attr: *const b::BarrierAttr,
    count: u32,
) -> c_int {
    if barrier.is_null() {
        return 22;
    }
    let a = if attr.is_null() {
        None
    } else {
        Some(unsafe { &*attr })
    };
    match b::init(unsafe { &mut *barrier }, a, count) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
