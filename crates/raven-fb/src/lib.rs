// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party Raspberry Pi firmware display buffer and HDMI text console.
//!
//! The crate is hardware-independent: it encodes the VideoCore property-buffer tags that ask the
//! firmware for a linear display buffer, decodes the firmware response, and renders monochrome
//! text into a caller-owned [`ScreenBuffer`]. The kernel supplies the mailbox registers, the
//! physical-address translation, the cache maintenance and the UART mirror; host tests exercise
//! the codec and the blitter without a Pi.
//!
//! The behaviour follows the documented Raspberry Pi mailbox property interface and the permissive
//! FreeBSD `bcm2835_mbox`/`bcm2835_fb` and NetBSD `bcm2835_genfb` drivers (see
//! `docs/hdmi-console.md`). No GPU, DRM or VideoCore command stream is involved.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod font;
pub mod mailbox;
pub mod pixel;
pub mod text;

pub use font::{FONT_HEIGHT, FONT_WIDTH, Glyph, glyph};
pub use mailbox::{
    DisplayInfo, DisplayRequest, MailboxError, PIXEL_ORDER_BGR, PIXEL_ORDER_RGB,
    TAG_ALLOCATE_BUFFER, TAG_GET_PITCH, TAG_SET_DEPTH, TAG_SET_PHYSICAL_WH, TAG_SET_PIXEL_ORDER,
    TAG_SET_VIRTUAL_WH, decode_response, encode_request,
};
pub use pixel::{Color, PixelFormat};
pub use text::{DirtyRows, ScreenBuffer, TextConsole, TextError};
