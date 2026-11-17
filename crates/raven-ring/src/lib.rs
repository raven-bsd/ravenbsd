// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure shared submission/completion ring core.
//!
//! Q1 models channels; this is the next layer: the fixed-capacity ring that carries descriptors
//! between the kernel and user space. It is the pure, dependency-free core of `R-IPC-21..22`; the
//! kernel object, the wire `SqEntry`/`CqEntry` layout and the buffer registration belong to Q6.
//!
//! # Model
//!
//! * [`Ring`] is a single-producer/single-consumer ring over caller-provided storage: the caller
//!   hands it a `&[AtomicU64]` slot slice (a power-of-two length) once, and the ring owns only the
//!   free-running `head`/`tail` counters and a poison flag, so the fast path never allocates.
//! * [`Descriptor`] packs a 63-bit value and a cancelled bit into one word; [`Ring::cancel`] marks
//!   pending descriptors so the consumer can report them without reordering the queue.
//! * [`Ring::poison`] puts the ring into a terminal error state: further submits fail and the
//!   consumer drains the remainder.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod ring;

pub use ring::{Descriptor, Error, Ring};
