// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The controller plan the kernel seam consumes.
//!
//! The plan is data only: the B54 device-tree parser produces the register window and the RP1
//! window, and the kernel seam maps the registers and runs [`crate::rp1::discover`]. Keeping the
//! plan separate from both keeps the device-tree parser out of the `no_std` kernel build.

use crate::ecam::Ecam;
use crate::region::Region;
use crate::rp1::Rp1Window;

/// One BCM2712 root complex, as described by the device tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciePlan {
    /// 4 KiB controller register block (`0x10_0012_0000 + 0x9310` in the B54 fixture).
    pub registers: Region,
    /// ECAM window, when the firmware tree declares one. The Pi 5 tree declares none.
    pub ecam: Option<Ecam>,
    /// RP1 child window, when the tree has an `rp1` node.
    pub rp1: Option<Rp1Window>,
}

impl PciePlan {
    /// Assemble a plan from already validated parts.
    #[must_use]
    pub const fn new(registers: Region, ecam: Option<Ecam>, rp1: Option<Rp1Window>) -> Self {
        Self {
            registers,
            ecam,
            rp1,
        }
    }

    /// The RP1 window, or `None` when this tree has no RP1 node.
    #[must_use]
    pub const fn rp1_window(&self) -> Option<Rp1Window> {
        self.rp1
    }
}

#[cfg(test)]
#[path = "plan_ut.rs"]
mod tests;
