// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RP1 enumeration, firmware-load and multi-function-device model.
//!
//! The Pi 5 RP1 controller is a PCIe endpoint with its own firmware and a set of child functions.
//! This crate carries the parts that do not need hardware:
//!
//! * [`firmware`] records what the official blob is (path, SHA-256, size, licence) without ever
//!   carrying the blob;
//! * [`frame`] decodes the mailbox response envelope through a caller-supplied [`frame::Layout`];
//! * [`transfer`] turns an image size and a chunk size into a deterministic chunk plan;
//! * [`machine`] is the enumeration and firmware-load state machine: identify, wait for the boot
//!   ROM, transfer chunk by chunk, verify, start, or fault;
//! * [`mfd`] reads the RP1 child windows from the device tree: the GPIO/pinctrl block, the clock
//!   provider and its assigned rates, the function windows and the firmware shared memory;
//! * [`gpio`] models pin ownership, pinctrl functions, direction and bias;
//! * [`clocks`] models the clock tree, rate limits and gate dependencies;
//! * [`power`] models power-domain ordering, users, resets and child dependencies.
//!
//! **No hardware claim.** The RP1 mailbox/register encoding and the pin-mux/power register layout
//! are not public. The frame layout is data supplied by the board bring-up, never guessed here,
//! and every transition is a model to be validated against a board log. The MFD addresses, clock
//! IDs and assigned rates are OBSERVED in the Pi 5 firmware device tree; `docs/pi5-rp1-mfd.md`
//! lists the UNVERIFIED steps.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod clocks;
pub mod firmware;
pub mod frame;
pub mod gpio;
pub mod machine;
pub mod mfd;
pub mod name;
pub mod power;
pub mod transfer;

pub use clocks::{ClockError, ClockSpec, ClockTree};
pub use firmware::{Digest, DigestError, FirmwareImage, UNKNOWN_LICENCE, UNKNOWN_PATH};
pub use frame::{Frame, FrameError, Layout, decode};
pub use gpio::{Bias, Direction, GpioError, PinConfig, PinController, PinRule, Pinctrl};
pub use machine::{
    Action, Config, ConfigError, Event, ExpectedIdentity, Fault, Identity, Limits, Machine,
    MachineError, State,
};
pub use mfd::{
    ClockBlock, FirmwareBlock, FunctionKind, FunctionWindow, GpioBlock, Interrupt, Mfd, MfdError,
};
pub use name::{Name, NameError};
pub use power::{Domain, DomainSpec, DomainState, PowerAction, PowerError, PowerModel};
pub use transfer::{Chunk, PlanError, Transfer, TransferError, TransferPlan};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
