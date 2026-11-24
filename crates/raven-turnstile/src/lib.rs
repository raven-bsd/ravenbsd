// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Turnstile wait queues and priority inheritance.
//!
//! A turnstile is the queue of threads blocked on one lock, ordered by effective priority then
//! arrival, plus the inheritance link to the owner. This crate is the *pure* model: it owns no
//! threads, locks or scheduler state, and never sleeps, spins or touches hardware. The kernel
//! (`L3`) hosts the real state and calls these operations; the host tests drive the same code
//! without a kernel, including an exhaustive small-state model check.
//!
//! Priorities follow `raven-sched-core`: `0` is the highest and larger numbers are lower. The
//! effective priority of a thread is the most urgent of its base priority and the effective
//! priorities of every thread waiting on a lock it owns, transitively (see [`Turnstiles`]).

#![cfg_attr(not(target_os = "none"), no_std)]
#![forbid(unsafe_code)]
// When compiled into the kernel (target_os = "none") the host-test-only public surface is unused.
#![cfg_attr(target_os = "none", allow(dead_code))]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod donation;
pub mod ipi_order;
mod turnstile;

pub use donation::{Donation, DonationError, DonationViolation};
#[cfg(feature = "lockstat")]
pub use turnstile::LockStats;
pub use turnstile::{
    CancelOutcome, Error, InheritEffect, InvariantViolation, ObjectId, Priority, ThreadId,
    Turnstiles,
};

#[cfg(test)]
#[path = "model_ut.rs"]
mod model_tests;
