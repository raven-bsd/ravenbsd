// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure probe and ring-buffer model for the RavenBSD DTrace provider.
//!
//! The kernel includes this crate's `lib.rs` through a `#[path]` attribute (the same pattern as
//! `raven-evq-core`) and hosts the real state; the host tests drive the same code without a kernel.
//! The ring is a single-producer/single-consumer lock-free queue: each CPU owns one ring and the
//! consumer serialises its drains.

#![cfg_attr(not(target_os = "none"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod probe;
pub mod ring;

pub use probe::{PROBES, Probe, ProbeInfo, probe};
pub use ring::{CpuStats, Record, Ring};
