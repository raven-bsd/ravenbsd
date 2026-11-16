// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the exec request packing.

use super::*;

#[test]
fn request_has_the_kernel_layout() {
    assert_eq!(core::mem::size_of::<ExecRequest>(), 88);
}

#[test]
fn pack_offsets_are_relative_to_the_string_block() {
    let mut strings = Vec::new();
    let mut offsets = Vec::new();
    pack(&mut strings, &[b"one", b"two"], &mut offsets).expect("pack");
    assert_eq!(strings, b"one\0two\0");
    assert_eq!(offsets, [0, 4]);
}

#[test]
fn pack_rejects_more_than_arg_max() {
    let big = vec![b'x'; MAX_ARG_BYTES];
    let mut strings = Vec::new();
    let mut offsets = Vec::new();
    assert_eq!(
        pack(&mut strings, &[big.as_slice()], &mut offsets),
        Err(Errno::E2BIG)
    );
}

#[test]
fn oversized_image_fails_before_the_syscall() {
    let image = vec![0u8; MAX_EXEC_IMAGE + 1];
    assert_eq!(exec_image(&image, b"/x", &[], &[], &[]), Err(Errno::EFBIG));
}

#[test]
fn oversized_path_is_rejected() {
    let path = vec![b'x'; MAX_PATH + 1];
    assert_eq!(
        exec_image(&[0u8], &path, &[], &[], &[]),
        Err(Errno::ENAMETOOLONG)
    );
}

#[test]
fn too_many_arguments_are_rejected() {
    let args = vec![&b""[..]; MAX_ARG_COUNT + 1];
    assert_eq!(
        exec_image(&[0u8], b"/x", &args, &[], &[]),
        Err(Errno::E2BIG)
    );
}
