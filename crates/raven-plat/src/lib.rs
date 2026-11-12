// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD platform-server core.
//!
//! The crate is host-testable and contains no MMIO or DMA access. [`mailbox`] owns the
//! VideoCore property-buffer codec and tag constants, [`gpio`] the pin arbiter, [`rng`] the
//! RNG200 health/read state machine and [`protocol`] the `plat:` path and call encoding. The
//! `platd` server supplies the hardware or fake [`platform::Platform`] backend.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod gpio;
pub mod mailbox;
pub mod platform;
pub mod protocol;
pub mod rng;

pub use gpio::{Function, GPIO_PINS, GpioArbiter, PinConfig, Pull};
pub use mailbox::{
    BoardInfo, FakeMailbox, MAX_PROPERTY_BYTES, Mailbox, TAG_GET_BOARD_MODEL,
    TAG_GET_BOARD_REVISION, TAG_GET_BOARDSERIAL, TAG_GET_CLOCKRATE, TAG_GET_MACADDRESS,
    TAG_GET_POWERSTATE,
};
pub use platform::{FakePlatform, Platform};
pub use protocol::{GPIO_LEVEL, GPIO_SET_FUNCTION, GPIO_SET_PULL, Resource, parse_resource};
pub use rng::{FakeRng, MAX_RNG_BYTES, Registers, Rng, Rng200};
