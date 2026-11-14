// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P4: pthread_create + pthread_join. Threads and futex (R-9, R-17).
#![no_std]
#![no_main]
use core::ffi::{c_int, c_void};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

type StartRoutine = extern "C" fn(*mut c_void) -> *mut c_void;

unsafe extern "C" {
    fn pthread_create(
        thread: *mut usize,
        attr: *const c_void,
        start: StartRoutine,
        arg: *mut c_void,
    ) -> c_int;
    fn pthread_join(thread: usize, value: *mut *mut c_void) -> c_int;
}

extern "C" fn start(_arg: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int {
    let mut thread = 0usize;
    unsafe {
        if pthread_create(
            &mut thread,
            core::ptr::null(),
            start as StartRoutine,
            core::ptr::null_mut(),
        ) != 0
        {
            return 1;
        }
        let _ = pthread_join(thread, core::ptr::null_mut());
    }
    0
}
