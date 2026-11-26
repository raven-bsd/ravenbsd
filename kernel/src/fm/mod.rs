// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel-side bounded ereport producer.
//!
//! The kernel only transports fixed-size ereports. Diagnosis, retire and degrade policy live in
//! the userland `fmd`; this module has no allocator, no locks and no policy.
//!
//! `wire.rs` is included from the shared protocol crate so the producer and consumer use one
//! validated schema without a kernel dependency edge. Integration adds `mod fm;` to the kernel
//! entry point; the Owns list for FMA1 stops at `kernel/src/fm/**`.

#[path = "../../../crates/raven-proto-fm/src/wire.rs"]
mod wire;

pub use wire::{EREPORT_LEN, Ereport, EreportRing, FmError, Severity};

/// The kernel producer queue capacity (fixed, no allocation).
pub const KERNEL_EREPORT_DEPTH: usize = 16;

/// The kernel-side producer/consumer queue.
pub type KernelEreportRing = EreportRing<KERNEL_EREPORT_DEPTH>;
