// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host assertion for S775-R01: the uutils `libc::statfs` record is the native FreeBSD
//! `struct statfs` that `libc/ravenlibc/crates/raven-libc/src/platform/statfs.rs` writes:
//! 2344 bytes, alignment 8, native field offsets. The pre-S775 record is 120 bytes.

use core::mem::{align_of, size_of};

use raven_statfs_abi::record::statfs;

#[test]
fn size_is_the_native_2344_bytes() {
    assert_eq!(size_of::<statfs>(), 2344);
}

#[test]
fn alignment_is_eight() {
    assert_eq!(align_of::<statfs>(), 8);
}
