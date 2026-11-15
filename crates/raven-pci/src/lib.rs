// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable BCM2712 PCIe root-complex and RP1 discovery model.
//!
//! The Pi 5 has a BCM2712 PCIe root complex with the RP1 I/O controller behind it. This crate
//! models the parts a host test can prove without hardware:
//!
//! * the controller register window, the optional ECAM window (the Pi 5 firmware device tree
//!   declares none) and the BCMstb indirect config-space protocol;
//! * typed config-space access (identity, class, header, command, capability walk) over an
//!   injected [`ConfigSpace`];
//! * BAR detection, sizing, assignment and a first-fit window allocator;
//! * RP1 discovery: find the endpoint whose memory BAR covers the RP1 `ranges` window that the
//!   B54 device-tree parser reports.
//!
//! Everything here is a register-level model. Real-device results are **UNVERIFIED** until a Pi 5
//! board log exists; `docs/pi5-pcie.md` records the split. The crate is `#![forbid(unsafe_code)]`
//! and `no_std` outside tests, so the kernel compiles the same source.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod bar;
pub mod bdf;
pub mod config;
pub mod ecam;
pub mod indirect;
pub mod plan;
pub mod region;
pub mod rp1;
pub mod sim;

pub use bar::{Bar, BarError, BarKind, BarSet, WindowAllocator};
pub use bdf::{Bdf, BdfError};
pub use config::{
    Class, ConfigSpace, Header, HeaderKind, Identity, enable_memory_and_bus_master, find_capability,
};
pub use ecam::{Ecam, EcamError};
pub use indirect::{Indirect, RegisterWindow};
pub use plan::PciePlan;
pub use region::Region;
pub use rp1::{DiscoverError, Rp1Device, Rp1Window, Scan, discover};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
