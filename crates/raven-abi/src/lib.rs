// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD native kernel ABI (version 1).
//!
//! This crate is a **contract**: the kernel, `raven-rt`, libc and every server depend on it. It is
//! owned by the lead; tickets must not edit it unless they say so explicitly.
//!
//! See `ravenbsd-prjmgmt/architecture/01-kernel.md` for the design.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod errno;
pub mod layout;
pub mod signal;
pub mod syscall;
pub mod types;
pub mod uname;

/// ABI version. Bumped on any incompatible change to numbers, layouts or semantics.
pub const ABI_VERSION: u32 = 1;

/// Size of a base page in bytes (all supported architectures use 4 KiB granules).
pub const PAGE_SIZE: usize = 4096;
