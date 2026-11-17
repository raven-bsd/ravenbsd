// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Physical frame allocators.
//!
//! The bitmap allocator lives in [`imp`] so the kernel can compile the same source directly with
//! `#[path]` while this crate remains the host-testable API. The buddy/zone allocator in [`buddy`]
//! is the newer power-of-two core (A5a) that the kernel's per-CPU frame path will sit on.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

mod buddy;
mod cache;
mod imp;

pub use buddy::{Block, BuddyAllocator, BuddyError, BuddyStats, FrameState, NoPager, Pager};
pub use cache::{CpuFrameStats, FrameCache, FrameSource};
pub use imp::*;
