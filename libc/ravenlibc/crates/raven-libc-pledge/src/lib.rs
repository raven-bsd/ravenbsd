// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pledge, unveil and Capsicum models for `ravenlibc`.
//!
//! This crate turns the OpenBSD promise grammar into a Raven capability plan and holds the
//! FreeBSD `cap_rights_*` bit operations. It is `no_std` plus `alloc`, class P and contains no
//! `unsafe` token.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)] // tests use fixed vectors and fail by panicking

extern crate alloc;

pub mod cap_rights;
pub mod capmap;
pub mod consts;
pub mod errors;
pub mod limits;
pub mod promise;
pub mod reduce;
pub mod unveil;

pub use errors::Errno;
