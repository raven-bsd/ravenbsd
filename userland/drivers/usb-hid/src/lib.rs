// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB HID boot-keyboard and boot-mouse driver service.
//!
//! This is the S62/S66 integration layer. The S60 [`raven_usb_hid::BootKeyboard`] model decodes
//! the boot keyboard reports and feeds the pressed-key ASCII into a bounded console input queue;
//! the [`raven_usb_hid::BootMouse`] model decodes the boot mouse reports (buttons, X/Y, wheel)
//! into a bounded [`PointerQueue`]. The queues are the seams a TTY/console or pointer consumer
//! will drain. Both services expose a detach/init re-enumeration path that releases stuck input
//! and stops repeat on removal. The real xHCI transfer ring and the bound TTY server are
//! follow-ups (S61 in Phil's lane).

#![no_std]

pub mod console;
pub mod mouse_service;
pub mod pointer;
pub mod repeat;
pub mod service;

pub use console::ConsoleInput;
pub use mouse_service::MouseService;
pub use pointer::{POINTER_QUEUE, PointerConsumer, PointerQueue};
pub use repeat::{Repeat, RepeatSettings};
pub use service::KeyboardService;

pub use raven_usb_hid::{
    BUTTON_BACK, BUTTON_FORWARD, BUTTON_LEFT, BUTTON_MIDDLE, BUTTON_RIGHT, BootMouse, Error,
    KeyEvent, KeyEvents, Layout, MOUSE_REPORT_LEN, Mouse, MouseError, MouseReport, PointerEvent,
    SetupPacket, TransferRing,
};

/// The `kbd:` instance path the console server opens for keyboard input.
pub const KBD_CONSOLE_PATH: &str = "console";

/// True when an `OPEN` path names the console keyboard instance.
///
/// The scheme root is also accepted because the service manager opens it for readiness; it never
/// carries bytes. The console consumer uses [`KBD_CONSOLE_PATH`].
pub fn accepts_console_path(path: &str) -> bool {
    let path = path.strip_prefix('/').unwrap_or(path);
    path.is_empty() || path == KBD_CONSOLE_PATH
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

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

/// The standard 52-byte four-byte (wheel) boot-mouse report descriptor.
///
/// It is the descriptor the fake USB core and the host tests hand to [`MouseService::init`]; a
/// three-byte boot mouse (no wheel) is also accepted by the model.
pub const BOOT_MOUSE_DESCRIPTOR: [u8; 52] = [
    0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
    0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01,
    0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08, 0x95, 0x03,
    0x81, 0x06, 0xc0, 0xc0,
];
