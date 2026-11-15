// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! BCM2712 PCIe root-complex and RP1 kernel seam.
//!
//! **UNVERIFIED.** This module has never run on a Pi 5. It is the kernel side of the
//! `raven-pci` model: map the controller register block, drive the BCMstb indirect config cycle
//! and discover the RP1 endpoint behind the root port. It deliberately does not touch the RP1
//! mailbox or load firmware; `raven-rp1` describes that protocol and a later ticket wires it once
//! a board log exists.
//!
//! The Pi 5 firmware device tree supplies the controller window (`0x10_0012_0000 + 0x9310` in the
//! B54 fixture) and the RP1 window. It supplies no ECAM range, so config space goes through the
//! indirect index/data registers. Both facts are device-tree OBSERVED and hardware UNVERIFIED.
//!
//! The module is declared by `kernel/src/arch/mod.rs` (a shared-additive wiring file) with
//! `#[path = "aarch64/pcie.rs"]`, because the architecture module tree itself is outside this
//! ticket's Owns paths.

// The Pi 5 platform path that calls the seam lands in a later ticket; until then the module is
// compiled but unused, and the seam is the deliverable.
#![allow(dead_code)]

use raven_pci::{DiscoverError, Indirect, Rp1Device, Rp1Window, Scan, discover};
use raven_rmm::MapError;

use crate::memory::mmio::map_mmio;

/// Bus the RP1 endpoint is expected to enumerate on. UNVERIFIED; a board log must confirm it.
pub const RP1_BUS: u8 = 1;

/// Why the seam cannot map or enumerate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcieError {
    /// The register window is empty or larger than the address space.
    Empty,
    /// The controller window does not fit the kernel address space.
    TooLarge,
    /// The kernel MMIO map refused the register window.
    Map(MapError),
    /// No unique RP1 endpoint answered.
    Discover(DiscoverError),
}

impl core::fmt::Display for PcieError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty PCIe register window"),
            Self::TooLarge => f.write_str("PCIe register window is too large"),
            Self::Map(error) => write!(f, "PCIe register map failed: {error}"),
            Self::Discover(error) => write!(f, "RP1 discovery failed: {error}"),
        }
    }
}

/// A mapped 32-bit controller register window.
///
/// The pointer stays private: every access is bounds checked against `length`, so a wrong offset
/// cannot read outside the mapped block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmioWindow {
    base: *mut u8,
    length: usize,
}

impl MmioWindow {
    /// Wrap an existing device mapping.
    ///
    /// # Safety
    ///
    /// `base` must point to a device-nGnRE (or equally side-effect-safe) mapping of at least
    /// `length` bytes that stays valid for the lifetime of this value, and `base` must be
    /// 4-byte aligned.
    #[must_use]
    pub unsafe fn new(base: *mut u8, length: usize) -> Option<Self> {
        if base.is_null() || length < 4 || base.align_offset(4) != 0 {
            return None;
        }
        Some(Self { base, length })
    }

    /// Mapped length in bytes.
    #[must_use]
    pub const fn length(self) -> usize {
        self.length
    }

    fn locate(self, offset: u32) -> Option<usize> {
        if offset & 3 != 0 {
            return None;
        }
        let start = offset as usize;
        let end = start.checked_add(4)?;
        if end > self.length {
            return None;
        }
        Some(start)
    }
}

impl raven_pci::RegisterWindow for MmioWindow {
    fn read32(&mut self, offset: u32) -> u32 {
        match self.locate(offset) {
            // SAFETY: `locate` proved a 4-byte aligned slot inside the mapping `new` accepted;
            // that mapping is device memory, so a volatile read is the only legal access.
            Some(start) => unsafe { core::ptr::read_volatile(self.base.add(start).cast::<u32>()) },
            // An out-of-range register access is a kernel bug; all ones keeps a caller from
            // interpreting a zero read as a valid response.
            None => u32::MAX,
        }
    }

    fn write32(&mut self, offset: u32, value: u32) {
        if let Some(start) = self.locate(offset) {
            // SAFETY: `locate` proved a 4-byte aligned slot inside the mapping and the block is
            // device memory; the volatile write is the intended side effect.
            unsafe {
                core::ptr::write_volatile(self.base.add(start).cast::<u32>(), value);
            }
        }
    }
}

/// Map the controller register block through the kernel's device-MMIO window.
///
/// # Errors
///
/// Returns [`PcieError::Empty`] for a zero-length block, [`PcieError::TooLarge`] when the block
/// does not fit a kernel address, or [`PcieError::Map`] when the kernel refuses the mapping.
pub fn map_controller(base: u64, length: u64) -> Result<MmioWindow, PcieError> {
    if length < 4 {
        return Err(PcieError::Empty);
    }
    let length = usize::try_from(length).map_err(|_| PcieError::TooLarge)?;
    let pointer = map_mmio(base, length).map_err(PcieError::Map)?;
    // SAFETY: `map_mmio` returned a device mapping of exactly `length` bytes at `pointer`; it is
    // page-aligned and therefore 4-byte aligned, and the mapping lives for the kernel's lifetime.
    unsafe { MmioWindow::new(pointer, length) }.ok_or(PcieError::Empty)
}

/// A config-space view over the BCMstb indirect registers.
pub type IndirectConfig = Indirect<MmioWindow>;

/// Map the controller and build its config-space backend.
///
/// # Errors
///
/// Returns the [`map_controller`] error.
pub fn config(base: u64, length: u64) -> Result<IndirectConfig, PcieError> {
    Ok(Indirect::new(map_controller(base, length)?))
}

/// Map the controller and find the RP1 endpoint.
///
/// `registers` is the controller register window from the device tree; `window` is the RP1
/// child window. The endpoint is expected on [`RP1_BUS`]; a board log must confirm the bus
/// number before this result is trusted.
///
/// # Errors
///
/// Returns [`PcieError::Map`] when the register block cannot be mapped and
/// [`PcieError::Discover`] when no unique endpoint covers the window.
pub fn enumerate_rp1(
    registers_base: u64,
    registers_length: u64,
    window: Rp1Window,
) -> Result<Rp1Device, PcieError> {
    let mut config = config(registers_base, registers_length)?;
    discover(&mut config, Scan::single(RP1_BUS), window).map_err(PcieError::Discover)
}
