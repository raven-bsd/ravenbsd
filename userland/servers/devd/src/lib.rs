// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable parts of `devd`.
//!
//! The `devd` program itself is a `no_std` service; the device-tree parser lives here so the
//! host tests can decode the committed `pi4-devd` fixture with `cargo test -p raven-devd`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod dtb;
pub mod pci;
pub mod platform;

#[cfg(test)]
mod dtb_ut;
#[cfg(test)]
mod pci_ut;
