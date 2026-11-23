// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! No-allocation ACPI table walker and Raven boot-bundle writer.
//!
//! The kernel compiles [`imp`] directly with `#[path]` on aarch64, so this crate stays the
//! host-testable API. It validates the RSDP and every table checksum, walks the RSDT/XSDT, and
//! writes the same `0x3143_4152` bundle the x86 boot path hands to `pcid`.
//!
//! The ACPI Revision 6.5 MCFG layout is implemented from the specification; the NetBSD
//! `sys/dev/acpi/acpi_mcfg.c` state machine is the behavioural donor. No donor code is copied.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

mod imp;

pub use imp::*;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
