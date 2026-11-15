// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RP1 discovery behind the BCM2712 root complex.
//!
//! The Pi 5 device tree describes the RP1 window as a child `ranges` entry: an RP1-local address
//! (`0xc0_4000_0000` in the B54 fixture), a PCI address (`0x0`), a CPU address
//! (`0x1f_0000_0000`) and a size (`0x41_0000`). The endpoint is the function whose memory BAR
//! contains the PCI window; the firmware programs that BAR before the OS runs (UNVERIFIED until
//! a board log exists).

use core::fmt;

use crate::bar::{Bar, BarSet};
use crate::bdf::Bdf;
use crate::config::{ConfigSpace, Identity, header, identity};
use crate::region::Region;

/// The three views of the RP1 window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rp1Window {
    local: Region,
    pci: Region,
    cpu: Region,
}

impl Rp1Window {
    /// Build the window from the device-tree values.
    ///
    /// `local_base` is the RP1-local child address, `pci_base` the address on the PCI bus,
    /// `cpu_base` the CPU physical address and `size` the window length. Every view must be a
    /// valid region.
    #[must_use]
    pub const fn new(local_base: u64, pci_base: u64, cpu_base: u64, size: u64) -> Option<Self> {
        let local = match Region::new(local_base, size) {
            Some(region) => region,
            None => return None,
        };
        let pci = match Region::new(pci_base, size) {
            Some(region) => region,
            None => return None,
        };
        let cpu = match Region::new(cpu_base, size) {
            Some(region) => region,
            None => return None,
        };
        Some(Self { local, pci, cpu })
    }

    /// RP1-local window.
    #[must_use]
    pub const fn local(self) -> Region {
        self.local
    }

    /// Window address on the PCI bus; the endpoint BAR must cover it.
    #[must_use]
    pub const fn pci(self) -> Region {
        self.pci
    }

    /// CPU physical window.
    #[must_use]
    pub const fn cpu(self) -> Region {
        self.cpu
    }

    /// Translate an RP1-local address to its CPU physical address.
    #[must_use]
    pub const fn translate(self, rp1_local: u64) -> Option<u64> {
        match self.local.offset_of(rp1_local) {
            Some(offset) => Some(self.cpu.base() + offset),
            None => None,
        }
    }
}

/// The bus range an enumeration pass probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    first_bus: u8,
    last_bus: u8,
}

impl Scan {
    /// A scan of `first_bus..=last_bus`, or `None` when the range is inverted.
    #[must_use]
    pub const fn buses(first_bus: u8, last_bus: u8) -> Option<Self> {
        if first_bus > last_bus {
            return None;
        }
        Some(Self {
            first_bus,
            last_bus,
        })
    }

    /// A scan of one bus. The Pi 5 firmware tree places RP1 behind the root port on bus 1
    /// (UNVERIFIED).
    #[must_use]
    pub const fn single(bus: u8) -> Self {
        Self {
            first_bus: bus,
            last_bus: bus,
        }
    }

    /// A scan of every bus.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            first_bus: 0,
            last_bus: 0xFF,
        }
    }

    /// First bus probed.
    #[must_use]
    pub const fn first_bus(self) -> u8 {
        self.first_bus
    }

    /// Last bus probed.
    #[must_use]
    pub const fn last_bus(self) -> u8 {
        self.last_bus
    }
}

/// The discovered RP1 function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rp1Device {
    /// Function address on the PCI bus.
    pub bdf: Bdf,
    /// Vendor and device IDs as read from config space.
    pub identity: Identity,
    /// The memory BAR that covers the RP1 window.
    pub bar: Bar,
    /// The device-tree window the BAR was matched against.
    pub window: Rp1Window,
}

impl Rp1Device {
    /// The BAR aperture, or `None` for a zero-sized BAR.
    #[must_use]
    pub const fn aperture(self) -> Option<Region> {
        self.bar.region()
    }
}

/// Why discovery found no unique RP1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoverError {
    /// No function decodes the RP1 window.
    NoDevice,
    /// More than one function decodes the window; the map is ambiguous.
    Ambiguous {
        /// The first match.
        first: Bdf,
        /// The second match.
        second: Bdf,
    },
}

impl fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => f.write_str("no PCI function decodes the RP1 window"),
            Self::Ambiguous { first, second } => write!(
                f,
                "two PCI functions decode the RP1 window: {first} and {second}"
            ),
        }
    }
}

impl core::error::Error for DiscoverError {}

/// Find the function whose memory BAR covers `window.pci()`.
///
/// Functions that do not respond, or whose BAR set is malformed, are skipped: a broken unrelated
/// device must not stop the scan. A second match is ambiguous and returns an error instead of
/// silently choosing the first one.
///
/// # Errors
///
/// Returns [`DiscoverError::NoDevice`] when nothing matches and [`DiscoverError::Ambiguous`] when
/// two functions do.
pub fn discover<C: ConfigSpace + ?Sized>(
    config: &mut C,
    scan: Scan,
    window: Rp1Window,
) -> Result<Rp1Device, DiscoverError> {
    let mut found: Option<Rp1Device> = None;
    for bus in scan.first_bus()..=scan.last_bus() {
        for device in 0u8..32 {
            let Ok(first) = Bdf::new(bus, device, 0) else {
                continue;
            };
            if identity(config, first).is_none() {
                continue;
            }
            let header = header(config, first);
            let functions: u8 = if header.multifunction { 8 } else { 1 };
            for function in 0u8..functions {
                if let Some(candidate) = probe_function(config, bus, device, function, window) {
                    if let Some(previous) = found {
                        return Err(DiscoverError::Ambiguous {
                            first: previous.bdf,
                            second: candidate.bdf,
                        });
                    }
                    found = Some(candidate);
                }
            }
        }
    }
    found.ok_or(DiscoverError::NoDevice)
}

fn probe_function<C: ConfigSpace + ?Sized>(
    config: &mut C,
    bus: u8,
    device: u8,
    function: u8,
    window: Rp1Window,
) -> Option<Rp1Device> {
    let bdf = Bdf::new(bus, device, function).ok()?;
    let id = identity(config, bdf)?;
    let bars = BarSet::read(config, bdf).ok()?;
    let bar = *bars.find_covering(window.pci())?;
    Some(Rp1Device {
        bdf,
        identity: id,
        bar,
        window,
    })
}

#[cfg(test)]
#[path = "rp1_ut.rs"]
mod tests;
