// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `compat-setmode`: the Rust `setmode`/`getmode` parser against the
//! OpenBSD donor C. The differential uses umask 0; the umask-sensitive rows are covered by the
//! crate's unit tests.

#![cfg(feature = "oracle-compat-setmode")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks
)]

use core::ffi::c_void;
use std::ffi::CString;

use raven_libc_compat::setmode::setmode;
use raven_libc_oracle::decls::compat_setmode::{oracle_getmode, oracle_setmode};

unsafe extern "C" {
    fn umask(mask: u32) -> u32;
}

#[test]
fn setmode_getmode_matches_the_donor() {
    // The donor `setmode` reads the process umask for the no-`who` form; pin it to 0 so the
    // differential compares the same rule the subject applies with `umask = 0`.
    let previous = unsafe { umask(0) };
    let cases: &[(&[u8], u32)] = &[
        (b"755", 0),
        (b"0755", 0),
        (b"0", 0o777),
        (b"7777", 0),
        (b"u+x", 0o644),
        (b"u-x", 0o755),
        (b"go-w", 0o777),
        (b"a+x", 0o644),
        (b"+x", 0o644),
        (b"-w", 0o666),
        (b"a=r", 0o777),
        (b"u=rwx,g=rx,o=", 0o777),
        (b"a=", 0o777),
        (b"=rw", 0o777),
        (b"u+s", 0o644),
        (b"g+s", 0o644),
        (b"+s", 0o644),
        (b"+t", 0o777),
        (b"g=u", 0o741),
        (b"o=u", 0o741),
        (b"u=g", 0o741),
        (b"g+u", 0o700),
    ];
    for &(spec, omode) in cases {
        let subject = setmode(spec).unwrap_or_else(|| panic!("subject setmode {spec:?}"));
        let subject_mode = subject.getmode(omode, 0);
        let cspec = CString::new(spec).expect("spec");
        let bbox = unsafe { oracle_setmode(cspec.as_ptr()) };
        assert!(!bbox.is_null(), "oracle setmode {spec:?}");
        let oracle_mode = unsafe { oracle_getmode(bbox as *const c_void, omode) };
        assert_eq!(subject_mode, oracle_mode, "spec {spec:?}");
    }
    unsafe { umask(previous) };
}

#[test]
fn invalid_specs_are_rejected_by_both() {
    let spec = CString::new("u+z").expect("spec");
    assert!(setmode(b"u+z").is_none());
    assert!(unsafe { oracle_setmode(spec.as_ptr()) }.is_null());
}
