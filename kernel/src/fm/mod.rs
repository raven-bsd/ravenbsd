// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel-side bounded ereport producer.
//!
//! The kernel only transports fixed-size ereports. Diagnosis, retire and degrade policy live in
//! the userland `fmd`; this module has no allocator, no locks and no policy. The queue is the
//! shared [`raven_proto_fm::EreportRing`] so the producer and consumer use one validated schema.

pub use raven_proto_fm::{EREPORT_LEN, Ereport, EreportRing, Severity};

/// The kernel producer queue capacity (fixed, no allocation).
pub const KERNEL_EREPORT_DEPTH: usize = 16;

/// The kernel-side producer/consumer queue.
pub type KernelEreportRing = EreportRing<KERNEL_EREPORT_DEPTH>;
