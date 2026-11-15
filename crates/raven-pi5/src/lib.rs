// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable Raspberry Pi 5 platform description.
//!
//! The Pi 5 is a BCM2712 SoC plus an RP1 I/O controller behind PCIe. This crate parses the
//! firmware device tree into typed data: the memory map, the BCM2712 UART/GIC/timers, the PCIe
//! root complex and the RP1 window. It also carries the host-side Pi 5 image-layout descriptor.
//!
//! No hardware is touched here: every value comes from a device tree, and every hardware-dependent
//! value remains UNVERIFIED until a board log exists. The parser is allocation-using `std` code
//! for the host, never `unsafe`, and returns [`Error`] instead of panicking on malformed input.

#![forbid(unsafe_code)]

mod image;
mod parse;

pub use image::{
    BootMode, CMDLINE, CONFIG_TXT, DTB, FIRMWARE_FILES, ImageLayout, KERNEL_CANDIDATES, PROFILE,
    layout,
};
pub use parse::{
    Chosen, Error, Gic, MemoryMap, MemoryRegion, Pcie, PcieRange, Pi5, Rp1, SystemTimer, Timer,
    Uart, UartSource, parse,
};

#[cfg(test)]
#[path = "../../../xtask/src/pi5.rs"]
mod xtask_pi5;
