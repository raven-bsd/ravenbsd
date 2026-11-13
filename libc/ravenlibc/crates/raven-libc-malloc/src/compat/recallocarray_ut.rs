// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn new_size_overflow_is_enomem() {
    let error = recalloc_sizes(1, usize::MAX / 2 + 1, 2).expect_err("overflow");
    assert_eq!(error, raven_libc_abi::errno::ENOMEM);
}

#[test]
fn old_size_overflow_is_einval() {
    let error = recalloc_sizes(usize::MAX / 2 + 1, 1, 2).expect_err("overflow");
    assert_eq!(error, raven_libc_abi::errno::EINVAL);
}

#[test]
fn null_old_pointer_has_no_old_size() {
    assert_eq!(recalloc_sizes(0, 4, 8), Ok((None, 32)));
}

#[test]
fn normal_sizes_round_trip() {
    assert_eq!(recalloc_sizes(3, 5, 8), Ok((Some(24), 40)));
}
