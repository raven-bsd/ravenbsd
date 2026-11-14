// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn null_object_and_optionals() {
    let value = 7u32;
    // SAFETY: a valid object.
    assert_eq!(*unsafe { obj_ref(&value) }, 7);
    // SAFETY: NULL is the documented optional absence.
    assert!(unsafe { opt::<u32>(core::ptr::null()) }.is_none());
    // SAFETY: as above.
    assert!(unsafe { opt_mut::<u32>(core::ptr::null_mut()) }.is_none());
}

#[test]
fn optional_views_see_the_value() {
    let value = 9u32;
    // SAFETY: a valid object.
    assert_eq!(unsafe { opt(&value) }.copied(), Some(9));
    let mut target = value;
    // SAFETY: a valid exclusive object.
    *unsafe { opt_mut(&mut target) }.expect("some") = 10;
    assert_eq!(target, 10);
}

#[test]
fn byte_views_respect_null_and_zero() {
    let data = [1u8, 2, 3];
    // SAFETY: NULL with zero is empty.
    assert!(unsafe { bytes(core::ptr::null(), 0) }.is_empty());
    // SAFETY: the array has three readable bytes.
    assert_eq!(unsafe { bytes(data.as_ptr().cast(), 3) }, &data);
    let mut out = [0u8; 3];
    // SAFETY: the array has three writable bytes.
    unsafe { bytes_mut(out.as_mut_ptr().cast(), 3) }.copy_from_slice(&[4, 5, 6]);
    assert_eq!(out, [4, 5, 6]);
}
