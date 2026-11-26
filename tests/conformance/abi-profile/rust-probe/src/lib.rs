// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compile-only Rust side of the S724 target-ABI oracle.
//!
//! The assertions are generated from `tests/conformance/abi-profile/libc-profile.json`
//! by `toolchain/gen-libc-consts.py emit-rust-probe`. Building this crate for
//! `x86_64-unknown-ravenbsd` and `aarch64-unknown-ravenbsd` checks the values and
//! record layouts of the vendored target `libc` crate itself, not the model tables.

#![no_std]

include!("generated_assertions.rs");

// FreeBSD rows the brush vendored nix FreeBSD path names that the S746 model does
// not freeze: the st_flags owner/superuser masks and B7200 (S765).
const _: () = {
    assert!(libc::UF_SETTABLE as u64 == 0x0000_ffff);
    assert!(libc::UF_NODUMP as u64 == 0x0000_0001);
    assert!(libc::UF_IMMUTABLE as u64 == 0x0000_0002);
    assert!(libc::UF_APPEND as u64 == 0x0000_0004);
    assert!(libc::UF_OPAQUE as u64 == 0x0000_0008);
    assert!(libc::UF_NOUNLINK as u64 == 0x0000_0010);
    assert!(libc::SF_SETTABLE as u64 == 0xffff_0000);
    assert!(libc::SF_ARCHIVED as u64 == 0x0001_0000);
    assert!(libc::SF_IMMUTABLE as u64 == 0x0002_0000);
    assert!(libc::SF_APPEND as u64 == 0x0004_0000);
    assert!(libc::SF_NOUNLINK as u64 == 0x0010_0000);
    assert!(libc::B7200 as u64 == 7200);
};
