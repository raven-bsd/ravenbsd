// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the C string helpers.

use super::*;
use core::ffi::c_char;

fn c(text: &[u8]) -> *const c_char {
    text.as_ptr().cast::<c_char>()
}

#[test]
fn a_null_pointer_is_the_empty_slice() {
    // SAFETY: a NULL pointer is explicitly allowed by the helper contract.
    assert!(unsafe { slice(core::ptr::null()) }.is_empty());
}

#[test]
fn reads_a_one_byte_string() {
    let text = b"x\0";
    // SAFETY: `text` is NUL terminated.
    assert_eq!(unsafe { slice(c(text)) }, b"x");
}

#[test]
fn reads_a_four_kibibyte_string() {
    let mut text = vec![b'a'; 4096];
    text.push(0);
    // SAFETY: `text` is NUL terminated.
    assert_eq!(unsafe { slice(c(&text)) }.len(), 4096);
}

#[test]
fn slice_n_stops_at_the_limit() {
    let text = b"abcdef\0";
    // SAFETY: five readable bytes.
    assert_eq!(unsafe { slice_n(c(text), 5) }, b"abcde");
}

#[test]
fn slice_n_stops_at_the_nul() {
    let text = b"abc\0xyz";
    // SAFETY: the NUL is before `n`.
    assert_eq!(unsafe { slice_n(c(text), 7) }, b"abc");
}

#[test]
fn a_null_with_zero_length_is_empty_not_a_dangling_slice() {
    // SAFETY: a zero length with a NULL pointer is the documented empty case.
    assert!(unsafe { slice_mut(core::ptr::null_mut(), 0) }.is_empty());
}

#[test]
fn slice_mut_exposes_the_buffer() {
    let mut buffer = [0u8; 3];
    // SAFETY: the array has three writable bytes.
    let view = unsafe { slice_mut(buffer.as_mut_ptr().cast::<c_char>(), 3) };
    view[0] = 7;
    assert_eq!(buffer[0], 7);
}

#[test]
fn bytes_n_keeps_embedded_nuls() {
    let data = [b'a', 0, b'b', 0, 7];
    // SAFETY: five readable bytes.
    assert_eq!(unsafe { bytes_n(c(&data), 5) }, &data);
}

#[test]
fn bytes_n_null_or_zero_is_empty() {
    // SAFETY: a NULL pointer and a zero length are the documented empty cases.
    assert!(unsafe { bytes_n(core::ptr::null(), 4) }.is_empty());
    // SAFETY: a zero length needs no readable bytes.
    assert!(unsafe { bytes_n(c(b"x"), 0) }.is_empty());
}

#[test]
fn checked_path_reads_an_in_window_string() {
    let text = b"abc\0";
    // SAFETY: `text` is NUL terminated and lies inside the user window.
    assert_eq!(unsafe { checked_path(c(text)) }, Ok(&b"abc"[..]));
}

#[test]
fn checked_path_is_efault_for_a_null_pointer() {
    // SAFETY: a NULL path is the documented EFAULT case.
    let result = unsafe { checked_path(core::ptr::null()) };
    assert_eq!(result, Err(Errno::from_generated(EFAULT)));
}

#[test]
fn checked_path_is_enametoolong_without_a_nul_within_path_max() {
    let text = vec![b'x'; PATH_MAX as usize];
    // SAFETY: the vector holds exactly `PATH_MAX` readable bytes and no NUL.
    let result = unsafe { checked_path(c(&text)) };
    assert_eq!(result, Err(Errno::from_generated(ENAMETOOLONG)));
}

/// The donor `t_mkdir.c:mkdir_err` pointer, `(void *)-1`.
///
/// The call must return `EFAULT` without ever dereferencing the pointer. It runs in a forked
/// child so that a missing check is a fatal signal in the child (the page fault on the current
/// path) and never kills the test process; the parent asserts the exact error through the
/// child's exit code.
#[test]
fn checked_path_rejects_the_donor_minus_one_pointer_with_efault() {
    // SAFETY: plain host libc declarations with the C signatures.
    unsafe extern "C" {
        fn fork() -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn _exit(status: i32) -> !;
    }

    // SAFETY: fork takes no pointers. nextest runs one test per process, so the child has no
    // other threads and only calls `checked_path` and `_exit`.
    let pid = unsafe { fork() };
    assert!(pid >= 0, "fork failed: {pid}");
    if pid == 0 {
        // SAFETY: `(void *)-1` is the donor invalid-pointer case; the helper must reject it
        // before walking the bytes, so this call must not touch the address.
        let result = unsafe { checked_path(usize::MAX as *const c_char) };
        let code = match result {
            Err(error) if error == Errno::from_generated(EFAULT) => 0,
            Err(_) => 1,
            Ok(_) => 2,
        };
        // SAFETY: ends the forked child without running the parent's test harness.
        unsafe { _exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is the child forked above and `status` is a live out pointer.
    let reaped = unsafe { waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid did not reap the checked_path child");
    let signal = status & 0x7f;
    assert_eq!(
        signal, 0,
        "checked_path((void *)-1) died from signal {signal}; it must return EFAULT"
    );
    let code = (status >> 8) & 0xff;
    assert_eq!(
        code, 0,
        "checked_path((void *)-1): 0 = EFAULT, 1 = wrong errno, 2 = accepted"
    );
}
