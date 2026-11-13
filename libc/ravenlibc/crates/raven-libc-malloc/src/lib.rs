// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc allocator core (area 08).
//!
//! Two interchangeable backends implement [`compat::Heap`]: [`hardened`] (the OpenBSD
//! `malloc.c` design) and [`tiny`] (the picolibc design). The crate is `no_std`, has no
//! `extern crate alloc` (R-8), and contains no `unsafe` outside [`raw`] (DR-39).
//!
//! Out of scope, by ticket: the C-ABI shell (C60/C64), `SysProvider` (C54), the mman wrappers
//! (C54), SSP (C50), thread locks and fork registry (C57), and `raven-umem`.

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]
// The allocator mirrors C's `size_t` arithmetic; every entry point validates lengths and
// alignments first, and the tests cover the boundaries. Keeping the donor's arithmetic visible is
// more auditable than rewriting each expression with wrapping/checked methods.
#![allow(clippy::arithmetic_side_effects)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
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
        clippy::undocumented_unsafe_blocks
    )
)]

pub mod api;
pub mod compat;
pub mod consts;
#[cfg(feature = "debug")]
pub mod debug;
pub mod errors;
#[cfg(feature = "hardened")]
pub mod hardened;
pub mod provider;
#[allow(unsafe_code)]
pub mod raw;
#[cfg(feature = "tiny")]
pub mod tiny;

/// Page-protection values passed to [`provider::PageProvider`].
pub mod map_prot {
    /// No access.
    pub const NONE: u8 = 0;
    /// Read.
    pub const READ: u8 = 1;
    /// Write.
    pub const WRITE: u8 = 2;
    /// Read and write.
    pub const READ_WRITE: u8 = READ | WRITE;
}

pub use api::Api;
pub use compat::aliases::LIBC_ALIASES;
pub use compat::checks::MAX_ALLOC;
pub use compat::options::Options;
pub use compat::{Heap, Mallinfo, Mallinfo2};
pub use consts::{PAGE_SHIFT, PAGE_SIZE};
#[cfg(feature = "hardened")]
pub use hardened::Hardened;
pub use provider::{Entropy, Lock, PageProvider, Platform};
#[cfg(feature = "tiny")]
pub use tiny::Tiny;
