// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-tested RP1 firmware-load handshake model.
//!
//! The Pi 5 boot path loads the official RP1 firmware before Ethernet/USB work. The RP1 boot ROM
//! and the host-side protocol are undocumented; this crate models the sequence a driver must
//! produce, behind a [`FirmwareSeam`] that the kernel seam will implement over the real window:
//!
//! * [`plan`] splits the image into bounded chunks;
//! * [`seam`] is the register/boot-ROM seam (reset, boot-ready poll, load-window write, doorbell,
//!   status and start);
//! * [`sim`] is an in-memory device double for the host tests;
//! * [`loader`] is the state machine: reset, wait for the boot ROM, write and ring each chunk,
//!   poll the status, verify/start, with timeout, busy and device-fault paths.
//!
//! **No hardware claim.** The real load-window offsets, doorbell encoding, status values, chunk
//! size and timeouts are UNVERIFIED. No RP1 firmware blob is present, copied or redistributed; the
//! tests use the synthetic fixture `tests/fixtures/pi5/rp1-fw-synthetic.md` and
//! [`REAL_FIRMWARE_PATH_UNVERIFIED`] records the missing board fact.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod loader;
pub mod plan;
pub mod seam;
pub mod sim;

pub use loader::{FwError, Limits, LoadOutcome, Loader, State, Step};
pub use plan::{Chunk, MAX_CHUNK, Plan, PlanError};
pub use seam::{FirmwareSeam, SeamError, Status};
pub use sim::SimSeam;

/// The real firmware path is a board fact that this workspace does not have.
pub const REAL_FIRMWARE_PATH_UNVERIFIED: &str =
    "UNVERIFIED: record the official RP1 firmware path, hash and licence on the bench";
/// This crate never redistributes the firmware blob.
pub const BLOB_REDISTRIBUTED: bool = false;

#[cfg(test)]
mod test_fixture;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
