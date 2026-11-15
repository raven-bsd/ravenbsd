// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The EMMC2 device-tree facts: translated base, interrupt and DMA window.
//!
//! The kernel already owns the live FDT. This module exists so the same lookup is host-tested
//! against a fixture and so the driver skeleton consumes typed data instead of parsing strings.
//! A missing node, a missing `reg`, an interrupt that is not a level-high SPI and a missing
//! `dma-ranges` window are all errors; nothing is guessed from a Pi 4 constant.

use core::fmt;

pub use raven_fdt_lite::DmaWindow;
use raven_fdt_lite::{Cells, Fdt, FdtError, Node};

/// Compatible string of the BCM2711 EMMC2 controller.
pub const EMMC2_COMPATIBLE: &str = "brcm,bcm2711-emmc2";

/// GIC interrupt type cell value for a shared peripheral interrupt.
pub const GIC_SPI: u64 = 0;
/// GIC interrupt flag cell value for level-high.
pub const GIC_LEVEL_HIGH: u64 = 4;
/// Highest GIC-400 shared peripheral interrupt number.
pub const GIC_MAX_SPI: u64 = 987;
/// The GIC id of SPI 0; a device-tree SPI number needs this offset.
pub const GIC_SPI_BASE: u32 = 32;

/// Why the EMMC2 node could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emmc2FdtError {
    /// The blob is not a readable device tree.
    Fdt(FdtError),
    /// No node declares [`EMMC2_COMPATIBLE`].
    MissingNode,
    /// The node has no translated `reg` entry.
    MissingReg,
    /// The `reg` entry has a zero length.
    EmptyReg,
    /// The `interrupts` property is absent or too short.
    MissingInterrupt,
    /// The `interrupts` property is not one level-high SPI.
    BadInterrupt {
        /// The interrupt-type cell.
        interrupt_type: u64,
        /// The SPI number cell.
        number: u64,
        /// The trigger/polarity flags cell.
        flags: u64,
    },
    /// No ancestor declares a non-empty `dma-ranges` window.
    MissingDmaWindow,
    /// The DMA window arithmetic overflows.
    BadDmaWindow {
        /// Device-visible base.
        bus_base: u64,
        /// CPU-physical base.
        cpu_base: u64,
        /// Window length.
        size: u64,
    },
}

impl fmt::Display for Emmc2FdtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fdt(error) => write!(f, "device tree: {error}"),
            Self::MissingNode => f.write_str("device tree has no brcm,bcm2711-emmc2 node"),
            Self::MissingReg => f.write_str("emmc2 node has no translated reg entry"),
            Self::EmptyReg => f.write_str("emmc2 reg entry is empty"),
            Self::MissingInterrupt => f.write_str("emmc2 node has no complete interrupts property"),
            Self::BadInterrupt {
                interrupt_type,
                number,
                flags,
            } => write!(
                f,
                "emmc2 interrupt <{interrupt_type} {number} {flags}> is not a level-high SPI"
            ),
            Self::MissingDmaWindow => f.write_str("emmc2 has no ancestor dma-ranges window"),
            Self::BadDmaWindow {
                bus_base,
                cpu_base,
                size,
            } => write!(
                f,
                "emmc2 dma window {bus_base:#x}/{cpu_base:#x}/{size:#x} overflows"
            ),
        }
    }
}

/// The EMMC2 node after parent `ranges` translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emmc2Node {
    /// CPU-physical base of the register window.
    pub base: u64,
    /// Register window length in bytes.
    pub size: u64,
    /// Zero-based GIC SPI number from the device tree.
    pub irq_spi: u32,
    /// DMA window of the nearest ancestor that declares `dma-ranges`.
    pub dma: DmaWindow,
}

impl Emmc2Node {
    /// The GIC interrupt id the kernel's `IRQ_CREATE` expects (SPI + 32).
    pub const fn gic_interrupt_id(self) -> u32 {
        self.irq_spi + GIC_SPI_BASE
    }

    /// Translate a CPU-physical DMA buffer address into the device's bus address.
    ///
    /// `None` when the buffer is outside the DTB window; the caller must then use a bounce buffer.
    pub fn bus_address(&self, cpu_physical: u64) -> Option<u64> {
        let offset = cpu_physical.checked_sub(self.dma.cpu_base)?;
        if offset >= self.dma.size {
            return None;
        }
        self.dma.bus_base.checked_add(offset)
    }

    /// One past the highest CPU-physical address the device can reach.
    pub fn cpu_limit(&self) -> Option<u64> {
        self.dma.cpu_base.checked_add(self.dma.size)
    }
}

/// Read the EMMC2 node from a flattened device tree.
///
/// The `reg` value is translated through every parent `ranges`; the DMA window is the first
/// non-empty `dma-ranges` entry of the nearest ancestor that declares one.
pub fn parse_emmc2(dtb: &[u8]) -> Result<Emmc2Node, Emmc2FdtError> {
    let fdt = Fdt::parse(dtb).map_err(Emmc2FdtError::Fdt)?;
    let node = fdt
        .find_compatible(&[EMMC2_COMPATIBLE])
        .ok_or(Emmc2FdtError::MissingNode)?;
    let (base, size) = node
        .reg_translated()
        .next()
        .ok_or(Emmc2FdtError::MissingReg)?;
    if size == 0 {
        return Err(Emmc2FdtError::EmptyReg);
    }

    let mut interrupts = node.cells("interrupts", 4);
    let interrupt_type = interrupts.next().ok_or(Emmc2FdtError::MissingInterrupt)?;
    let number = interrupts.next().ok_or(Emmc2FdtError::MissingInterrupt)?;
    let flags = interrupts.next().ok_or(Emmc2FdtError::MissingInterrupt)?;
    if interrupt_type != GIC_SPI || number > GIC_MAX_SPI || flags != GIC_LEVEL_HIGH {
        return Err(Emmc2FdtError::BadInterrupt {
            interrupt_type,
            number,
            flags,
        });
    }
    let irq_spi = u32::try_from(number).map_err(|_| Emmc2FdtError::BadInterrupt {
        interrupt_type,
        number,
        flags,
    })?;

    let dma = nearest_dma_window(node).ok_or(Emmc2FdtError::MissingDmaWindow)?;
    if dma.size == 0
        || dma.bus_base.checked_add(dma.size).is_none()
        || dma.cpu_base.checked_add(dma.size).is_none()
    {
        return Err(Emmc2FdtError::BadDmaWindow {
            bus_base: dma.bus_base,
            cpu_base: dma.cpu_base,
            size: dma.size,
        });
    }

    Ok(Emmc2Node {
        base,
        size,
        irq_spi,
        dma,
    })
}

fn nearest_dma_window(node: Node<'_>) -> Option<DmaWindow> {
    let mut current = node.parent();
    while let Some(bus) = current {
        if let Some(window) = first_dma_window(bus) {
            return Some(window);
        }
        current = bus.parent();
    }
    None
}

fn first_dma_window(bus: Node<'_>) -> Option<DmaWindow> {
    let value = bus.property("dma-ranges")?;
    if value.is_empty() {
        return None;
    }
    let child_cells = bus.address_cells();
    let parent_cells = bus.parent().map_or(2, |parent| parent.address_cells());
    let size_cells = bus.size_cells();
    if child_cells > 2 || parent_cells > 2 || size_cells > 2 {
        return None;
    }
    let mut cells = bus.cells("dma-ranges", 4);
    let bus_base = read_cells(&mut cells, child_cells)?;
    let cpu_base = read_cells(&mut cells, parent_cells)?;
    let size = read_cells(&mut cells, size_cells)?;
    (size != 0).then_some(DmaWindow {
        bus_base,
        cpu_base,
        size,
    })
}

fn read_cells(cells: &mut Cells<'_>, count: u32) -> Option<u64> {
    let mut value = 0u64;
    for _ in 0..count {
        value = value.checked_shl(32)?.checked_add(cells.next()?)?;
    }
    Some(value)
}

#[cfg(test)]
#[path = "fdt_ut.rs"]
mod tests;
