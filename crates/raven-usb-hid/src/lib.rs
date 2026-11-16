// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-keyboard core.
//!
//! The crate is the host-tested half of the Pi USB keyboard path: HID class/report descriptor
//! parsing, the USB HID boot-protocol report (8 bytes: modifiers, one reserved byte, six key
//! slots), the HID usage-to-ASCII map with shift/caps/control state, and a [`device::BootKeyboard`]
//! that drives a [`device::TransferRing`] seam. The xHCI/RP1 transfer-ring implementation and the
//! real enumeration are the follow-up in Phil's lane; this crate makes no hardware claim.

#![no_std]

pub mod descriptor;
pub mod device;
pub mod keymap;
pub mod report;
pub mod setup;

pub use descriptor::{
    Field, HidDescriptor, MAX_FIELDS, ReportLayout, parse_hid_descriptor, parse_report_descriptor,
};
pub use device::{BootKeyboard, REPORT_ENDPOINT, TransferRing};
pub use keymap::key_to_ascii;
pub use report::{BOOT_REPORT_LEN, BootReport, KeyEvent, KeyEvents, Keyboard, MAX_EVENTS};
pub use setup::SetupPacket;

/// Errors from the descriptor, report and transfer-ring layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A descriptor or report is shorter than its declared size.
    TooShort,
    /// An item uses an unsupported size or type.
    BadItem,
    /// The report descriptor does not describe a boot keyboard.
    NotAKeyboard,
    /// A boot report was shorter than eight bytes.
    ShortReport,
    /// The device model was used before `init`.
    NotInitialized,
    /// The transfer-ring backend has no device or transfer available.
    NoDevice,
}
