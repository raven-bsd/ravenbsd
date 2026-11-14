// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn null_argv_is_empty() {
    // SAFETY: NULL is the documented empty array.
    assert_eq!(unsafe { scan(core::ptr::null()) }, Ok((0, 0)));
    // SAFETY: NULL is the documented empty array.
    assert_eq!(
        // SAFETY: NULL is the documented empty array.
        unsafe { collect(core::ptr::null()) },
        Ok(alloc::vec::Vec::new())
    );
}

#[test]
fn collects_two_arguments() {
    let first = b"one\0";
    let second = b"two\0";
    let pointers: [*const core::ffi::c_char; 3] = [
        first.as_ptr().cast(),
        second.as_ptr().cast(),
        core::ptr::null(),
    ];
    // SAFETY: the array is NULL-terminated and the strings are NUL-terminated.
    let arguments = unsafe { collect(pointers.as_ptr()) }.expect("valid array");
    assert_eq!(arguments, alloc::vec![b"one".to_vec(), b"two".to_vec()]);
    // SAFETY: as above.
    assert_eq!(unsafe { scan(pointers.as_ptr()) }, Ok((2, 8)));
}

#[test]
fn walk_visits_every_string() {
    let first = b"a\0";
    let pointers: [*const core::ffi::c_char; 2] = [first.as_ptr().cast(), core::ptr::null()];
    let mut seen = alloc::vec::Vec::new();
    // SAFETY: the array and strings follow the contract.
    unsafe {
        walk(pointers.as_ptr(), &mut |value| {
            seen.push(value.to_vec());
            Ok(())
        })
    }
    .expect("valid array");
    assert_eq!(seen, alloc::vec![b"a".to_vec()]);
}

#[test]
fn unterminated_array_is_e2big() {
    let word = b"x\0";
    let pointers = alloc::vec![word.as_ptr().cast::<core::ffi::c_char>(); MAX_ARGS + 2];
    // SAFETY: every pointer is a valid NUL-terminated string; the array is longer than the
    // accepted count and has no terminator inside the bound.
    let error = unsafe { scan(pointers.as_ptr()) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::E2BIG);
}

#[test]
fn more_than_arg_max_bytes_is_e2big() {
    let big = alloc::vec![b'x'; (ARG_MAX / 2) + 1];
    let mut first = big.clone();
    first.push(0);
    let mut second = big;
    second.push(0);
    let pointers: [*const core::ffi::c_char; 3] = [
        first.as_ptr().cast(),
        second.as_ptr().cast(),
        core::ptr::null(),
    ];
    // SAFETY: two valid strings whose total exceeds ARG_MAX.
    let error = unsafe { scan(pointers.as_ptr()) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::E2BIG);
}
