// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// P8: qsort + bsearch + strtol + regcomp/regexec. Regex weight.
#![no_std]
#![no_main]
use core::ffi::{c_char, c_int, c_void};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

const REG_EXTENDED: c_int = 1;

unsafe extern "C" {
    fn qsort(base: *mut c_void, count: usize, size: usize, compare: extern "C" fn(*const c_void, *const c_void) -> c_int);
    fn bsearch(
        key: *const c_void,
        base: *const c_void,
        count: usize,
        size: usize,
        compare: extern "C" fn(*const c_void, *const c_void) -> c_int,
    ) -> *mut c_void;
    fn strtol(text: *const c_char, end: *mut *mut c_char, base: c_int) -> i64;
    fn regcomp(regex: *mut c_void, pattern: *const c_char, flags: c_int) -> c_int;
    fn regexec(regex: *const c_void, text: *const c_char, nmatch: usize, matches: *mut c_void, flags: c_int) -> c_int;
    fn regfree(regex: *mut c_void);
}

extern "C" fn compare(left: *const c_void, right: *const c_void) -> c_int {
    let left = unsafe { *(left.cast::<i32>()) };
    let right = unsafe { *(right.cast::<i32>()) };
    if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char, _envp: *const *const c_char) -> c_int {
    let mut values = [3i32, 1, 2];
    let mut end: *mut c_char = core::ptr::null_mut();
    let mut regex = [0u8; 64];
    unsafe {
        qsort(values.as_mut_ptr().cast(), values.len(), 4, compare);
        let _ = bsearch(
            values.as_ptr().cast(),
            values.as_ptr().cast(),
            values.len(),
            4,
            compare,
        );
        let text = if argc > 0 {
            *argv
        } else {
            c"1".as_ptr()
        };
        let _ = strtol(text, &mut end, 10);
        if regcomp(regex.as_mut_ptr().cast(), c"a+".as_ptr(), REG_EXTENDED) == 0 {
            let _ = regexec(
                regex.as_ptr().cast(),
                c"aaa".as_ptr(),
                0,
                core::ptr::null_mut(),
                0,
            );
            regfree(regex.as_mut_ptr().cast());
        }
    }
    argc
}
