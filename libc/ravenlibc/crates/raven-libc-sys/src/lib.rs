// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `raven-libc-sys`: the only crate in ravenlibc that executes a Raven system call.
//!
//! The crate owns the per-architecture `syscall0..6` stubs (copied once from
//! `userland/rt/src/sys.rs`, same project, BSD-3-Clause), one safe typed wrapper per
//! `raven_abi::syscall::Syscall` variant, the `errno` cell and its TLS provider seam, the
//! thread-pointer accessors, the C8 auxv reader, cancellation wrappers, the `PageProvider`
//! raw implementation and the deterministic `host-fake` kernel used by host tests.
//!
//! `raven-libc-sys` has no dependency on `raven-libc-tls` or `raven-rt`; the TLS layout
//! crate installs the errno provider through [`errno::install_errno_slot`].

#![no_std]
#![allow(unsafe_code)] // raw system-call stubs and thread-pointer access
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
        clippy::arithmetic_side_effects,
        clippy::undocumented_unsafe_blocks
    )
)]

#[cfg(any(test, feature = "host-fake"))]
extern crate std as host;

pub mod arch;
pub mod auxv;
#[cfg(feature = "cancel")]
pub mod cancel;
pub mod errno;
#[cfg(feature = "host-fake")]
pub mod fake;
pub mod macros;
pub mod pages;
pub mod pending;
pub mod sys;
pub mod tp;
pub mod unimpl;

/// The ABI revision this crate was compiled against.
pub const ABI_VERSION: u32 = raven_abi::ABI_VERSION;

/// `true` when the Raven system-call numbers (`0x01..=0x100`) are valid.
#[inline]
pub const fn is_raven_syscall_number(nr: usize) -> bool {
    nr >= 0x01 && nr <= 0x100
}
