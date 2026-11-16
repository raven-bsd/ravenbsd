// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-keyboard driver service: report decoding into a console input queue.
//!
//! This is the S62 integration layer. The S60 [`raven_usb_hid::BootKeyboard`] model decodes the
//! boot reports; this crate feeds the pressed-key ASCII into a bounded console input queue. The
//! queue is the seam a TTY/console consumer will drain; the real xHCI transfer ring and the
//! bound TTY server are follow-ups (S61 in Phil's lane).

#![no_std]

pub mod console;
pub mod repeat;
pub mod service;

pub use console::ConsoleInput;
pub use repeat::{Repeat, RepeatSettings};
pub use service::KeyboardService;

pub use raven_usb_hid::{Error, Layout, SetupPacket, TransferRing};

/// The standard 63-byte boot-keyboard report descriptor.
///
/// It is the descriptor the fake USB core and the bring-up tests hand to
/// [`KeyboardService::init`]; the real device reports its own descriptor over
/// `GET_DESCRIPTOR(0x22)`, and a driver fetches it through [`TransferRing::control_in`].
pub const BOOT_KEYBOARD_DESCRIPTOR: [u8; 63] = [
    0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x06, 0x75, 0x08,
    0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0x05, 0x08, 0x19, 0x01,
    0x29, 0x05, 0x95, 0x05, 0x75, 0x01, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x01, 0xc0,
];
