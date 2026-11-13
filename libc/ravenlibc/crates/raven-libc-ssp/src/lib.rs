// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc stack-protector runtime and FORTIFY check cores (area 08 3.15-3.16).
//!
//! The exported C symbols, the `__stack_chk_guard` storage and the variadic `*_chk` wrappers
//! belong to the shell (C60). This crate is pure logic: canary seeding, the failure sequence,
//! the overlap test and one check core per FreeBSD `*_chk.c` file. It is `no_std`,
//! allocation-free and `forbid(unsafe_code)`.

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
)]

pub mod chk;
pub mod fail;
pub mod guard;
pub mod overlap;

pub use chk::ChkFail;
pub use fail::{FailHooks, MSG_CHK, MSG_STACK, chk_fail, stack_chk_fail};
pub use guard::{
    CanarySink, GUARD, GuardSource, SSP_WEAK, compute_guard, compute_guard_width, init_guard,
};
pub use overlap::ssp_overlap;
