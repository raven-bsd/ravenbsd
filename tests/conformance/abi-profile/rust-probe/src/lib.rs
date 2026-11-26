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
