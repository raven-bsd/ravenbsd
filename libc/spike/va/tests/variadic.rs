// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests: our stable-Rust `my_snprintf` versus the host libc `snprintf`.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

use raven_va_spike::my_snprintf;

unsafe extern "C" {
    fn snprintf(buf: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
}

fn assert_same(ours: &[u8], host: &[u8], fmt: &str, ours_len: i32, host_len: i32) {
    assert_eq!(ours_len, host_len, "return length for {fmt:?}");
    // SAFETY: the buffer is NUL-terminated by the implementation before it returns.
    let ours_string = unsafe { CStr::from_ptr(ours.as_ptr().cast::<c_char>()) };
    // SAFETY: the host snprintf NUL-terminates the buffer.
    let host_string = unsafe { CStr::from_ptr(host.as_ptr().cast::<c_char>()) };
    assert_eq!(ours_string, host_string, "output for {fmt:?}");
}

macro_rules! check {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {{
        let format = CString::new($fmt).expect("test format has no NUL");
        let mut ours = [0u8; 512];
        let mut host = [0u8; 512];
        // SAFETY: the test format and arguments match, and the buffer is writable.
        let ours_len = unsafe {
            my_snprintf(
                ours.as_mut_ptr(),
                ours.len(),
                format.as_ptr().cast::<u8>(),
                $($arg),*
            )
        };
        // SAFETY: the same arguments satisfy the host snprintf contract.
        let host_len = unsafe {
            snprintf(
                host.as_mut_ptr().cast::<c_char>(),
                host.len(),
                format.as_ptr(),
                $($arg),*
            )
        };
        assert_same(&ours, &host, $fmt, ours_len, host_len);
    }};
}

#[test]
fn strings_chars_and_percent() {
    check!("hello");
    check!("%%");
    check!("%c", i32::from(b'Z'));
    check!("%5c", i32::from(b'x'));
    check!("%-5c|", i32::from(b'y'));
    check!("%s", c"world".as_ptr());
    check!("%10s", c"world".as_ptr());
    check!("%-10s|", c"world".as_ptr());
    check!("%.3s", c"truncate".as_ptr());
    check!("%10.3s", c"truncate".as_ptr());
    check!("%s", ptr::null::<c_char>());
    check!("%p", ptr::null::<c_void>());
}

#[test]
fn integers_and_flags() {
    check!("%d", 0i32);
    check!("%d", -42i32);
    check!("%i", 1234i32);
    check!("%u", 4000000000u32);
    check!("%x", 0xdead_beefu32);
    check!("%X", 0xdead_beefu32);
    check!("%o", 0o755u32);
    check!("%8d", 42i32);
    check!("%-8d|", 42i32);
    check!("%08d", -42i32);
    check!("%+d", 42i32);
    check!("% d", 42i32);
    check!("%.6d", 42i32);
    check!("%8.6d", 42i32);
    check!("%08.6d", 42i32);
    check!("%u", 0u32);
    check!("%.0u", 0u32);
    check!("%ld", -9_000_000_000i64);
    check!("%lu", 9_000_000_000u64);
    check!("%lld", i64::MIN);
    check!("%llu", u64::MAX);
    check!("%zu", usize::MAX);
}

#[test]
fn float_formats() {
    check!("%f", 0.0f64);
    check!("%f", 1.5f64);
    check!("%f", -2.25f64);
    check!("%.0f", 1.5f64);
    check!("%.2f", 1234.5678f64);
    check!("%8.2f", 3.5f64);
    check!("%-8.2f|", 3.5f64);
    check!("%08.2f", 3.5f64);
    check!("%+.2f", 3.5f64);
    check!("% .2f", 3.5f64);
    check!("%f", -0.0f64);
    check!("%f", f64::INFINITY);
    check!("%f", f64::NEG_INFINITY);
    check!("%f", f64::NAN);
    check!("%.10f", 1.0f64 / 3.0f64);
}

#[test]
fn overflow_area_and_mixed_registers() {
    check!(
        "%d %d %d %d %d %d %d %d %d %d",
        1i32,
        2i32,
        3i32,
        4i32,
        5i32,
        6i32,
        7i32,
        8i32,
        9i32,
        10i32
    );
    check!(
        "%d:%f %d:%f %d:%f %d:%f %d:%f %d:%f",
        1i32,
        1.5f64,
        2i32,
        2.5f64,
        3i32,
        3.5f64,
        4i32,
        4.5f64,
        5i32,
        5.5f64,
        6i32,
        6.5f64
    );
    check!(
        "%d %d %d %d %d %d %d %d %f %f",
        11i32,
        12i32,
        13i32,
        14i32,
        15i32,
        16i32,
        17i32,
        18i32,
        1.25f64,
        2.75f64
    );
}

#[test]
fn pointers_and_strings() {
    let value = 0x1234_5678u32;
    let pointer = ptr::addr_of!(value).cast::<c_void>();
    check!("%p", pointer);
    check!("%20p", pointer);
    let text = CString::new("hello").expect("no NUL");
    check!("%s %s", text.as_ptr(), text.as_ptr());
    check!("%.0s", text.as_ptr());
    check!("%5.2s", text.as_ptr());
}

#[test]
fn truncation_and_zero_capacity() {
    let format = CString::new("value=%d").expect("no NUL");
    let mut ours = [0u8; 4];
    let mut host = [0u8; 4];
    // SAFETY: the test format and arguments match, and the buffer is writable.
    let ours_len = unsafe {
        my_snprintf(
            ours.as_mut_ptr(),
            ours.len(),
            format.as_ptr().cast::<u8>(),
            12345i32,
        )
    };
    // SAFETY: the same arguments satisfy the host snprintf contract.
    let host_len = unsafe {
        snprintf(
            host.as_mut_ptr().cast::<c_char>(),
            host.len(),
            format.as_ptr(),
            12345i32,
        )
    };
    assert_eq!(ours_len, host_len);
    assert_eq!(ours, host);

    // SAFETY: n = 0 with a null buffer is permitted; the format and argument are valid.
    let ours_len =
        unsafe { my_snprintf(ptr::null_mut(), 0, format.as_ptr().cast::<u8>(), 12345i32) };
    // SAFETY: n = 0 with a null buffer is permitted; the format and argument are valid.
    let host_len = unsafe { snprintf(ptr::null_mut(), 0, format.as_ptr(), 12345i32) };
    assert_eq!(ours_len, host_len);
    assert_eq!(ours_len, 11);
}
