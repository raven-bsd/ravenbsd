// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Device-tree platform-device discovery.
//!
//! `devd` reads the platform bus from the firmware device tree. This module turns the flattened
//! tree into [`DeviceSpec`] values with CPU physical `reg` windows, the inherited DMA windows and
//! the GIC interrupt numbers; [`match_drivers`] then binds `/etc/drivers.toml` entries by their
//! `compatible` string.
//!
//! The parser allocates only the device list; the FDT itself is borrowed for the call. A node
//! outside every parent `ranges` window is an error rather than a silently skipped device, so a
//! bad tree cannot produce a driver bound to the wrong address.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_fdt_lite::{DmaWindow, Fdt, FdtError, Node};

/// Deepest node nesting the walk accepts; real trees are far shallower.
const MAX_DEPTH: usize = 32;

/// One MMIO window in the CPU physical address space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegWindow {
    /// First byte of the window.
    pub base: u64,
    /// Window length in bytes.
    pub size: u64,
}

/// GIC interrupt type encoded in the first `interrupts` cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptKind {
    /// Shared peripheral interrupt.
    Spi,
    /// Private peripheral interrupt.
    Ppi,
}

/// One interrupt decoded from a GIC `interrupts` specifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GicInterrupt {
    /// SPI or PPI.
    pub kind: InterruptKind,
    /// Number as written in the device tree (the GIC SPI/PPI index, not the INTID).
    pub number: u32,
    /// Trigger type and level flags from the third cell.
    pub trigger: u32,
}

impl GicInterrupt {
    /// Short `spi=126` / `ppi=5` label used in log lines.
    pub fn label(&self) -> String {
        let kind = match self.kind {
            InterruptKind::Spi => "spi",
            InterruptKind::Ppi => "ppi",
        };
        format!("{kind}={}", self.number)
    }
}

/// One bindable platform device decoded from the device tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSpec {
    /// Absolute node path, for example `/scb/ethernet@7d580000`.
    pub path: String,
    /// Every `compatible` string, in tree order.
    pub compatibles: Vec<String>,
    /// `reg` windows translated to CPU physical addresses.
    pub regs: Vec<RegWindow>,
    /// DMA windows inherited from the nearest ancestor with `dma-ranges`; empty means identity.
    pub dma: Vec<DmaWindow>,
    /// GIC interrupts in tree order.
    pub interrupts: Vec<GicInterrupt>,
}

impl DeviceSpec {
    /// The first `compatible` string, which is the most specific one.
    pub fn compatible(&self) -> Option<&str> {
        self.compatibles.first().map(String::as_str)
    }

    /// First `reg` window, or a zero window when the node has none.
    pub fn first_reg(&self) -> Option<RegWindow> {
        self.regs.first().copied()
    }

    /// `spi=157,spi=158` style label, or `none`.
    pub fn irq_label(&self) -> String {
        if self.interrupts.is_empty() {
            return "none".to_string();
        }
        let mut label = String::new();
        for interrupt in &self.interrupts {
            if !label.is_empty() {
                label.push(',');
            }
            label.push_str(&interrupt.label());
        }
        label
    }
}

/// Why a device tree could not produce device specs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtbError {
    /// The blob is not a valid flattened device tree.
    Fdt(FdtError),
    /// The tree is deeper than the parser's fixed parent scan.
    TooDeep,
    /// A `compatible` property is malformed or empty.
    BadCompatible,
    /// A `reg` property is not a whole number of cells.
    BadReg,
    /// A `reg` address is outside the parent `ranges` windows.
    UnmappedReg,
    /// A node has `interrupts` but no inherited `interrupt-parent`.
    MissingInterruptParent,
    /// The `interrupt-parent` does not name an enabled GIC interrupt controller.
    BadInterruptParent,
    /// The `interrupts` property is truncated or uses an unknown GIC type.
    BadInterrupt,
}

impl core::fmt::Display for DtbError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DtbError::Fdt(error) => write!(formatter, "device tree: {error}"),
            DtbError::TooDeep => formatter.write_str("device tree is too deep"),
            DtbError::BadCompatible => formatter.write_str("malformed compatible property"),
            DtbError::BadReg => formatter.write_str("malformed reg property"),
            DtbError::UnmappedReg => formatter.write_str("reg is outside the parent ranges"),
            DtbError::MissingInterruptParent => {
                formatter.write_str("interrupts without an interrupt-parent")
            }
            DtbError::BadInterruptParent => {
                formatter.write_str("interrupt-parent is not a GIC interrupt controller")
            }
            DtbError::BadInterrupt => formatter.write_str("malformed GIC interrupts property"),
        }
    }
}

impl From<FdtError> for DtbError {
    fn from(error: FdtError) -> Self {
        DtbError::Fdt(error)
    }
}

/// Decode every enabled node with both a `compatible` and a `reg` property.
///
/// Nodes are returned in depth-first tree order. A node with `status = "disabled"` (or `"fail"`)
/// is skipped together with its subtree, as the device-tree specification requires.
pub fn parse(blob: &[u8]) -> Result<Vec<DeviceSpec>, DtbError> {
    let fdt = Fdt::parse(blob)?;
    let root = fdt.root().ok_or(DtbError::Fdt(FdtError::BadToken))?;
    let mut devices = Vec::new();
    walk(&fdt, root, 0, &mut devices)?;
    Ok(devices)
}

/// Match `drivers.toml` entries to devices by their first equal `compatible` string.
///
/// Entries without `compatible` are PCI entries and are ignored here. The first device in tree
/// order wins when several nodes share a compatible string.
pub fn match_drivers<'a>(
    devices: &'a [DeviceSpec],
    drivers: &'a [raven_proto_dev::DriverSpec],
) -> Vec<(&'a DeviceSpec, &'a raven_proto_dev::DriverSpec)> {
    let mut matched = Vec::new();
    for driver in drivers {
        let Some(compatible) = driver.compatible.as_deref() else {
            continue;
        };
        if let Some(device) = devices
            .iter()
            .find(|device| device.compatibles.iter().any(|value| value == compatible))
        {
            matched.push((device, driver));
        }
    }
    matched
}

fn walk(
    fdt: &Fdt<'_>,
    node: Node<'_>,
    depth: usize,
    devices: &mut Vec<DeviceSpec>,
) -> Result<(), DtbError> {
    if depth > MAX_DEPTH {
        return Err(DtbError::TooDeep);
    }
    if !is_enabled(&node) {
        return Ok(());
    }
    if node.property("compatible").is_some() && node.property("reg").is_some() {
        devices.push(decode_device(fdt, node, depth)?);
    }
    for child in node.children() {
        walk(fdt, child, depth.saturating_add(1), devices)?;
    }
    Ok(())
}

fn is_enabled(node: &Node<'_>) -> bool {
    !matches!(
        node.property_str("status"),
        Some("disabled" | "fail" | "fail-sss")
    )
}

fn decode_device(fdt: &Fdt<'_>, node: Node<'_>, depth: usize) -> Result<DeviceSpec, DtbError> {
    let path = node_path(node, depth)?;
    let compatibles = decode_compatibles(node)?;
    let regs = decode_regs(node)?;
    let dma = decode_dma(fdt, &path);
    let interrupts = decode_interrupts(fdt, node)?;
    Ok(DeviceSpec {
        path,
        compatibles,
        regs,
        dma,
        interrupts,
    })
}

fn node_path(node: Node<'_>, depth: usize) -> Result<String, DtbError> {
    let mut components = Vec::new();
    let mut current = Some(node);
    let mut levels = depth;
    while let Some(node) = current {
        if levels > MAX_DEPTH {
            return Err(DtbError::TooDeep);
        }
        let name = node.name();
        if !name.is_empty() {
            components.push(name);
        }
        current = node.parent();
        levels = levels.saturating_add(1);
    }
    let mut path = String::new();
    for component in components.iter().rev() {
        path.push('/');
        path.push_str(component);
    }
    if path.is_empty() {
        path.push('/');
    }
    Ok(path)
}

fn decode_compatibles(node: Node<'_>) -> Result<Vec<String>, DtbError> {
    let value = node.property("compatible").ok_or(DtbError::BadCompatible)?;
    let mut compatibles = Vec::new();
    for entry in value.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let text = core::str::from_utf8(entry).map_err(|_| DtbError::BadCompatible)?;
        compatibles.push(text.to_string());
    }
    if compatibles.is_empty() {
        return Err(DtbError::BadCompatible);
    }
    Ok(compatibles)
}

fn decode_regs(node: Node<'_>) -> Result<Vec<RegWindow>, DtbError> {
    let parent = node.parent().ok_or(DtbError::BadReg)?;
    let address_cells = parent.address_cells();
    let size_cells = parent.size_cells();
    if address_cells > 2 || size_cells > 2 {
        return Err(DtbError::BadReg);
    }
    let mut regs = Vec::new();
    for (address, size) in node
        .reg_checked(address_cells, size_cells)
        .map_err(|_| DtbError::BadReg)?
    {
        let base =
            raven_fdt_lite::translate_address(&node, address).ok_or(DtbError::UnmappedReg)?;
        regs.push(RegWindow { base, size });
    }
    if regs.is_empty() {
        return Err(DtbError::BadReg);
    }
    Ok(regs)
}

fn decode_dma(fdt: &Fdt<'_>, path: &str) -> Vec<DmaWindow> {
    let mut windows = [DmaWindow::EMPTY; 8];
    let Some(count) = fdt.dma_windows_for(path, &mut windows) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for window in windows.iter().take(count) {
        result.push(*window);
    }
    result
}

fn decode_interrupts(fdt: &Fdt<'_>, node: Node<'_>) -> Result<Vec<GicInterrupt>, DtbError> {
    let Some(value) = node.property("interrupts") else {
        return Ok(Vec::new());
    };
    if value.is_empty() || !value.len().is_multiple_of(12) {
        return Err(DtbError::BadInterrupt);
    }
    let phandle = interrupt_parent(node).ok_or(DtbError::MissingInterruptParent)?;
    let controller = find_phandle(fdt, phandle).ok_or(DtbError::BadInterruptParent)?;
    if !is_gic(&controller)
        || controller.u32_property("#interrupt-cells") != Some(3)
        || controller.property("interrupt-controller").is_none()
    {
        return Err(DtbError::BadInterruptParent);
    }
    let mut cells = node.cells("interrupts", 4);
    let mut interrupts = Vec::new();
    while let Some(kind) = cells.next() {
        let number = cells.next().ok_or(DtbError::BadInterrupt)?;
        let trigger = cells.next().ok_or(DtbError::BadInterrupt)?;
        let kind = match kind {
            0 => InterruptKind::Spi,
            1 => InterruptKind::Ppi,
            _ => return Err(DtbError::BadInterrupt),
        };
        let number = u32::try_from(number).map_err(|_| DtbError::BadInterrupt)?;
        let trigger = u32::try_from(trigger).map_err(|_| DtbError::BadInterrupt)?;
        interrupts.push(GicInterrupt {
            kind,
            number,
            trigger,
        });
    }
    Ok(interrupts)
}

fn interrupt_parent(node: Node<'_>) -> Option<u32> {
    let mut current = Some(node);
    while let Some(node) = current {
        if let Some(value) = node.property("interrupt-parent") {
            let bytes: [u8; 4] = value.try_into().ok()?;
            return Some(u32::from_be_bytes(bytes));
        }
        current = node.parent();
    }
    None
}

fn find_phandle<'a>(fdt: &Fdt<'a>, phandle: u32) -> Option<Node<'a>> {
    let root = fdt.root()?;
    find_phandle_in(root, phandle, 0)
}

fn find_phandle_in<'a>(node: Node<'a>, phandle: u32, depth: usize) -> Option<Node<'a>> {
    if depth > MAX_DEPTH {
        return None;
    }
    for property in ["phandle", "linux,phandle"] {
        let Some(value) = node.property(property) else {
            continue;
        };
        let bytes: [u8; 4] = value.try_into().ok()?;
        if u32::from_be_bytes(bytes) == phandle {
            return Some(node);
        }
    }
    for child in node.children() {
        if let Some(found) = find_phandle_in(child, phandle, depth.saturating_add(1)) {
            return Some(found);
        }
    }
    None
}

fn is_gic(node: &Node<'_>) -> bool {
    ["arm,gic-400", "arm,cortex-a15-gic", "arm,gic-v3"]
        .iter()
        .any(|name| node.compatible_with(name))
}
