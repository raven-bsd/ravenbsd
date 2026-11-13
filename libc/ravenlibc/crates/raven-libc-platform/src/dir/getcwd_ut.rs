// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getcwd` argument tests (COPY row 6).

use raven_abi::errno::Errno;

use crate::dir::getcwd::getcwd;

#[test]
fn buf_null_size_zero_allocates_as_needed() {
    let result = getcwd(b"/tmp", None, 0).expect("alloc").expect("buffer");
    assert_eq!(result, b"/tmp\0");
    assert_eq!(result.len(), 5);
}

#[test]
fn buf_null_with_large_size_allocates_that_size() {
    let result = getcwd(b"/tmp", None, 64).expect("alloc").expect("buffer");
    assert_eq!(&result[..5], b"/tmp\0");
    assert_eq!(result.len(), 64);
}

#[test]
fn buf_with_size_zero_is_einval() {
    let mut buffer = [0u8; 8];
    assert_eq!(getcwd(b"/tmp", Some(&mut buffer), 0), Err(Errno::EINVAL));
}

#[test]
fn too_small_buffer_is_erange() {
    let mut buffer = [0u8; 4];
    assert_eq!(getcwd(b"/tmp", Some(&mut buffer), 4), Err(Errno::ERANGE));
}

#[test]
fn exact_buffer_is_filled() {
    let mut buffer = [0u8; 5];
    assert_eq!(getcwd(b"/tmp", Some(&mut buffer), 5), Ok(None));
    assert_eq!(&buffer, b"/tmp\0");
}

#[test]
fn larger_buffer_is_zero_terminated() {
    let mut buffer = [0xaau8; 16];
    getcwd(b"/tmp", Some(&mut buffer), 16).expect("getcwd");
    assert_eq!(&buffer[..5], b"/tmp\0");
    assert_eq!(buffer[5], 0xaa);
}

#[test]
fn root_is_one_slash() {
    let mut buffer = [0u8; 2];
    getcwd(b"/", Some(&mut buffer), 2).expect("getcwd");
    assert_eq!(&buffer, b"/\0");
}
