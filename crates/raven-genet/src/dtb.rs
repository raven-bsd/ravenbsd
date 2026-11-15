// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pi 4 device-tree path for the GENET node.
//!
//! The parser reads the live firmware tree rather than hard-coding board addresses: the GENET
//! `reg` is translated through `/scb` `ranges`, the MDIO register offset comes from the child
//! node, the PHY address and mode come from the PHY node, and the MAC property is optional
//! (the firmware may patch it at boot; the mailbox is the fallback). The `/scb` `dma-ranges`
//! windows are kept as data so the driver can place packet buffers without assuming identity.

use core::fmt;

use raven_fdt_lite::{DmaWindow, Fdt, FdtError, Node};

use crate::mac::MacAddress;
use crate::phy::PhyMode;
use crate::regs::WINDOW_SIZE;

/// Largest number of `dma-ranges` windows the parser keeps.
pub const MAX_DMA_WINDOWS: usize = 8;
/// Deepest node path the fixed buffer accepts.
pub const MAX_NODE_DEPTH: usize = 16;
/// Capacity of the fixed node-path buffer.
pub const MAX_PATH_BYTES: usize = 160;
/// Number of hardware interrupts the GENET node carries.
pub const GENET_INTERRUPTS: usize = 2;

/// The DMA windows of the nearest ancestor that declares `dma-ranges`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaMap {
    windows: [DmaWindow; MAX_DMA_WINDOWS],
    len: usize,
    identity: bool,
}

impl DmaMap {
    /// An identity mapping (no `dma-ranges` property, or an empty one).
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            windows: [DmaWindow::EMPTY; MAX_DMA_WINDOWS],
            len: 0,
            identity: true,
        }
    }

    /// Copy the first [`MAX_DMA_WINDOWS`] windows.
    #[must_use]
    pub fn from_windows(windows: &[DmaWindow]) -> Self {
        let count = windows.len().min(MAX_DMA_WINDOWS);
        let mut map = Self {
            windows: [DmaWindow::EMPTY; MAX_DMA_WINDOWS],
            len: count,
            identity: count == 0,
        };
        if let (Some(destination), Some(source)) =
            (map.windows.get_mut(..count), windows.get(..count))
        {
            destination.copy_from_slice(source);
        }
        map
    }

    /// The declared windows, empty for an identity mapping.
    #[must_use]
    pub fn as_slice(&self) -> &[DmaWindow] {
        self.windows.get(..self.len).unwrap_or(&[])
    }

    /// True when the map is the identity mapping.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        self.identity
    }

    /// Translate a device-visible bus address to a CPU physical address.
    #[must_use]
    pub fn translate_bus(&self, bus: u64) -> Option<u64> {
        if self.identity {
            return Some(bus);
        }
        for window in self.as_slice() {
            let Some(end) = window.bus_base.checked_add(window.size) else {
                continue;
            };
            if bus >= window.bus_base && bus < end {
                return window.cpu_base.checked_add(bus - window.bus_base);
            }
        }
        None
    }

    /// Translate a CPU physical address to a device-visible bus address.
    #[must_use]
    pub fn translate_cpu(&self, cpu: u64) -> Option<u64> {
        if self.identity {
            return Some(cpu);
        }
        for window in self.as_slice() {
            let Some(end) = window.cpu_base.checked_add(window.size) else {
                continue;
            };
            if cpu >= window.cpu_base && cpu < end {
                return window.bus_base.checked_add(cpu - window.cpu_base);
            }
        }
        None
    }
}

impl Default for DmaMap {
    fn default() -> Self {
        Self::identity()
    }
}

/// The data the driver needs from the GENET and MDIO device-tree nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenetInfo {
    /// CPU physical base of the register window.
    pub base: u64,
    /// Register window length.
    pub size: u64,
    /// MDIO register offset from [`GenetInfo::base`].
    pub mdio_offset: u32,
    /// MDIO register window length.
    pub mdio_size: u32,
    /// GIC SPI numbers of the two GENET interrupts.
    pub interrupts: [u32; GENET_INTERRUPTS],
    /// Interrupt trigger/level flags, one per entry in [`GenetInfo::interrupts`].
    pub interrupt_flags: [u32; GENET_INTERRUPTS],
    /// PHY address on the MDIO bus.
    pub phy_address: u8,
    /// RGMII delay mode.
    pub phy_mode: PhyMode,
    /// The address patched into the device tree, when present.
    pub mac: Option<MacAddress>,
    /// DMA windows from the nearest ancestor with `dma-ranges`.
    pub dma: DmaMap,
}

impl GenetInfo {
    /// CPU physical address of the MDIO register.
    #[must_use]
    pub fn mdio_base(&self) -> Option<u64> {
        self.base.checked_add(self.mdio_offset as u64)
    }
}

/// Why the device tree does not describe a usable GENET node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtbError {
    /// The blob is not a supported FDT.
    Fdt(FdtError),
    /// The tree has no root node.
    MissingRoot,
    /// No enabled GENET v5 node was found.
    MissingGenet,
    /// The GENET `reg` is absent, malformed or empty.
    BadReg,
    /// The GENET `reg` is outside every parent `ranges` window.
    UnmappedReg,
    /// The register window is smaller than the GENET block.
    WindowTooSmall(u64),
    /// Fewer than two GIC interrupts are described.
    MissingInterrupts,
    /// An interrupt is not a three-cell GIC SPI specifier.
    BadInterrupt,
    /// The MDIO child node is absent.
    MissingMdio,
    /// The MDIO `reg` is malformed or outside the GENET window.
    BadMdio,
    /// The PHY child node is absent.
    MissingPhy,
    /// The PHY address is malformed or out of range.
    BadPhy,
    /// The `phy-mode` property is absent.
    MissingPhyMode,
    /// The `phy-mode` property is not one of the four RGMII modes.
    BadPhyMode,
    /// A MAC property is present but is not six bytes.
    BadMac,
    /// The node path did not fit the fixed buffer.
    PathTooLong,
}

impl fmt::Display for DtbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fdt(error) => write!(formatter, "device tree: {error}"),
            Self::MissingRoot => formatter.write_str("device tree has no root node"),
            Self::MissingGenet => formatter.write_str("no enabled GENET v5 node"),
            Self::BadReg => formatter.write_str("bad GENET reg"),
            Self::UnmappedReg => formatter.write_str("GENET reg is outside the parent ranges"),
            Self::WindowTooSmall(size) => write!(formatter, "GENET window is only {size:#x} bytes"),
            Self::MissingInterrupts => {
                formatter.write_str("GENET node has fewer than two interrupts")
            }
            Self::BadInterrupt => formatter.write_str("GENET interrupt is not a GIC SPI"),
            Self::MissingMdio => formatter.write_str("GENET node has no MDIO child"),
            Self::BadMdio => formatter.write_str("bad MDIO reg"),
            Self::MissingPhy => formatter.write_str("MDIO node has no PHY child"),
            Self::BadPhy => formatter.write_str("bad PHY address"),
            Self::MissingPhyMode => formatter.write_str("GENET node has no phy-mode"),
            Self::BadPhyMode => formatter.write_str("unsupported phy-mode"),
            Self::BadMac => formatter.write_str("MAC property is not six bytes"),
            Self::PathTooLong => formatter.write_str("device-tree node path is too long"),
        }
    }
}

impl From<FdtError> for DtbError {
    fn from(error: FdtError) -> Self {
        Self::Fdt(error)
    }
}

/// Parse the Pi 4 GENET node from a firmware device tree.
pub fn parse(data: &[u8]) -> Result<GenetInfo, DtbError> {
    let fdt = Fdt::parse(data)?;
    let root = fdt.root().ok_or(DtbError::MissingRoot)?;
    let node = find_enabled(root, &["brcm,bcm2711-genet-v5", "brcm,genet-v5"])
        .ok_or(DtbError::MissingGenet)?;

    let parent = node.parent().ok_or(DtbError::BadReg)?;
    let mut reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(|_| DtbError::BadReg)?;
    let (address, size) = reg.next().ok_or(DtbError::BadReg)?;
    if size < WINDOW_SIZE as u64 {
        return Err(DtbError::WindowTooSmall(size));
    }
    let base = raven_fdt_lite::translate_address(&node, address).ok_or(DtbError::UnmappedReg)?;

    let (interrupts, interrupt_flags) = parse_interrupts(&node)?;
    let phy_mode = node
        .property_str("phy-mode")
        .ok_or(DtbError::MissingPhyMode)
        .and_then(|value| PhyMode::from_dt(value).ok_or(DtbError::BadPhyMode))?;
    let mac = parse_mac(&node)?;

    let mdio = node
        .children()
        .find(|child| {
            enabled(child)
                && (child.compatible_with("brcm,genet-mdio-v5") || child.name().starts_with("mdio"))
        })
        .ok_or(DtbError::MissingMdio)?;
    let mut mdio_reg = mdio
        .reg_checked(
            mdio_parent_address_cells(&mdio),
            mdio_parent_size_cells(&mdio),
        )
        .map_err(|_| DtbError::BadMdio)?;
    let (mdio_offset, mdio_size) = mdio_reg.next().ok_or(DtbError::BadMdio)?;
    let mdio_offset = u32::try_from(mdio_offset).map_err(|_| DtbError::BadMdio)?;
    let mdio_size = u32::try_from(mdio_size).map_err(|_| DtbError::BadMdio)?;
    if mdio_size == 0 || u64::from(mdio_offset) + u64::from(mdio_size) > size {
        return Err(DtbError::BadMdio);
    }

    let phy = mdio
        .children()
        .find(|child| {
            enabled(child)
                && (child.name().starts_with("ethernet-phy@")
                    || child.compatible_with("ethernet-phy-ieee802.3-c22"))
        })
        .ok_or(DtbError::MissingPhy)?;
    let mut phy_reg = phy
        .reg_checked(mdio.address_cells(), mdio.size_cells())
        .map_err(|_| DtbError::BadPhy)?;
    let (phy_address, _) = phy_reg.next().ok_or(DtbError::BadPhy)?;
    let phy_address = u8::try_from(phy_address).map_err(|_| DtbError::BadPhy)?;
    if phy_address > crate::mdio::MAX_PHY_ADDRESS {
        return Err(DtbError::BadPhy);
    }

    let dma = parse_dma(&fdt, &node)?;

    Ok(GenetInfo {
        base,
        size,
        mdio_offset,
        mdio_size,
        interrupts,
        interrupt_flags,
        phy_address,
        phy_mode,
        mac,
        dma,
    })
}

fn mdio_parent_address_cells(mdio: &Node<'_>) -> u32 {
    mdio.parent().map_or(1, |parent| parent.address_cells())
}

fn mdio_parent_size_cells(mdio: &Node<'_>) -> u32 {
    mdio.parent().map_or(1, |parent| parent.size_cells())
}

fn parse_interrupts(
    node: &Node<'_>,
) -> Result<([u32; GENET_INTERRUPTS], [u32; GENET_INTERRUPTS]), DtbError> {
    let mut cells = node.cells("interrupts", 4);
    let mut numbers = [0u32; GENET_INTERRUPTS];
    let mut flags = [0u32; GENET_INTERRUPTS];
    for index in 0..GENET_INTERRUPTS {
        let kind = cells.next().ok_or(DtbError::MissingInterrupts)?;
        let number = cells.next().ok_or(DtbError::MissingInterrupts)?;
        let cell_flags = cells.next().ok_or(DtbError::MissingInterrupts)?;
        if kind != 0 {
            return Err(DtbError::BadInterrupt);
        }
        numbers[index] = u32::try_from(number).map_err(|_| DtbError::BadInterrupt)?;
        flags[index] = u32::try_from(cell_flags).map_err(|_| DtbError::BadInterrupt)?;
    }
    Ok((numbers, flags))
}

fn parse_mac(node: &Node<'_>) -> Result<Option<MacAddress>, DtbError> {
    for name in ["local-mac-address", "mac-address"] {
        if let Some(value) = node.property(name) {
            if value.len() != crate::mac::MAC_ADDRESS_LEN {
                return Err(DtbError::BadMac);
            }
            let mut bytes = [0u8; crate::mac::MAC_ADDRESS_LEN];
            bytes.copy_from_slice(value);
            return Ok(Some(MacAddress::new(bytes)));
        }
    }
    Ok(None)
}

fn parse_dma(fdt: &Fdt<'_>, node: &Node<'_>) -> Result<DmaMap, DtbError> {
    let mut path = [0u8; MAX_PATH_BYTES];
    let length = node_path(node, &mut path).ok_or(DtbError::PathTooLong)?;
    let path = core::str::from_utf8(path.get(..length).ok_or(DtbError::PathTooLong)?)
        .map_err(|_| DtbError::PathTooLong)?;
    let mut windows = [DmaWindow::EMPTY; MAX_DMA_WINDOWS];
    match fdt.dma_windows_for(path, &mut windows) {
        Some(count) => Ok(DmaMap::from_windows(
            windows.get(..count).ok_or(DtbError::PathTooLong)?,
        )),
        None => Ok(DmaMap::identity()),
    }
}

fn node_path(node: &Node<'_>, out: &mut [u8]) -> Option<usize> {
    let mut parts = [""; MAX_NODE_DEPTH];
    let mut count = 0usize;
    let mut current = Some(*node);
    while let Some(item) = current {
        if !item.name().is_empty() {
            *parts.get_mut(count)? = item.name();
            count += 1;
        }
        current = item.parent();
    }
    let mut length = 1usize;
    *out.get_mut(0)? = b'/';
    for index in (0..count).rev() {
        let name = parts.get(index)?.as_bytes();
        if length > 1 {
            *out.get_mut(length)? = b'/';
            length += 1;
        }
        let end = length.checked_add(name.len())?;
        out.get_mut(length..end)?.copy_from_slice(name);
        length = end;
    }
    Some(length)
}

fn find_enabled<'a>(node: Node<'a>, names: &[&str]) -> Option<Node<'a>> {
    if names.iter().any(|name| node.compatible_with(name)) && enabled(&node) {
        return Some(node);
    }
    for child in node.children() {
        if let Some(found) = find_enabled(child, names) {
            return Some(found);
        }
    }
    None
}

fn enabled(node: &Node<'_>) -> bool {
    !matches!(
        node.property_str("status"),
        Some("disabled" | "reserved" | "fail" | "fail-sss")
    )
}

#[cfg(test)]
#[path = "dtb_ut.rs"]
mod tests;
