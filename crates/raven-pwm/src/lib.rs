// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable Raspberry Pi 4 PWM1 audio model.
//!
//! The Pi 4 3.5 mm jack is driven by PWM1 at `0xfe20c800`, not the legacy PWM0 block. This crate
//! owns the register and bit fields, the CPRMAN clock plan, the GPIO40/41 pinmux arithmetic, the
//! S16LE-to-duty conversion and the 32-byte DMA control-block ring. It never touches MMIO or DMA;
//! `userland/drivers/pwm-audio` provides the board seam and consumes the `audio:` PCM ring.
//!
//! Every board-specific value is a documented fact or marked UNVERIFIED in `docs/pi4-audio.md`;
//! no hardware result is claimed by the host tests.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
extern crate std;

pub mod clock;
pub mod dma;
pub mod duty;
pub mod gpio;
pub mod registers;

#[cfg(test)]
#[path = "dtb_ut.rs"]
mod dtb_tests;
