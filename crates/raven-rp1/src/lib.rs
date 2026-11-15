// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RP1 enumeration and firmware-load model.
//!
//! The Pi 5 RP1 controller is a PCIe endpoint with its own firmware. This crate carries the parts
//! that do not need hardware:
//!
//! * [`firmware`] records what the official blob is (path, SHA-256, size, licence) without ever
//!   carrying the blob;
//! * [`frame`] decodes the mailbox response envelope through a caller-supplied [`frame::Layout`];
//! * [`transfer`] turns an image size and a chunk size into a deterministic chunk plan;
//! * [`machine`] is the enumeration and firmware-load state machine: identify, wait for the boot
//!   ROM, transfer chunk by chunk, verify, start, or fault.
//!
//! **No hardware claim.** The RP1 mailbox/register encoding is not public. The frame layout is
//! data supplied by the board bring-up, never guessed here, and every transition is a model to be
//! validated against a board log. `docs/pi5-pcie.md` lists the UNVERIFIED steps.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod firmware;
pub mod frame;
pub mod machine;
pub mod transfer;

pub use firmware::{Digest, DigestError, FirmwareImage, UNKNOWN_LICENCE, UNKNOWN_PATH};
pub use frame::{Frame, FrameError, Layout, decode};
pub use machine::{
    Action, Config, ConfigError, Event, ExpectedIdentity, Fault, Identity, Limits, Machine,
    MachineError, State,
};
pub use transfer::{Chunk, PlanError, Transfer, TransferError, TransferPlan};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
