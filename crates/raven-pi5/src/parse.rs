// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Device-tree to Pi 5 platform description.
//!
//! The documented node values come from the official Pi 5 firmware DTB and the NetBSD/Linux
//! `bcm2712` DTS sources; see `tests/fixtures/pi5/README.md` for the citations. The parser never
//! trusts a length it did not check and never panics on malformed input.

use core::fmt;

use raven_fdt_lite::{Fdt, FdtError, Node};

/// One physical memory range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    /// First physical address.
    pub base: u64,
    /// Length in bytes.
    pub size: u64,
}

impl MemoryRegion {
    /// One-past-the-end address, or `None` on overflow.
    #[must_use]
    pub const fn end(&self) -> Option<u64> {
        self.base.checked_add(self.size)
    }
}

/// The usable memory map and the reserved-memory ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryMap {
    /// `/memory` regions, sorted by base.
    pub regions: Vec<MemoryRegion>,
    /// `/reserved-memory` regions that carry a `reg`, sorted by base.
    pub reserved: Vec<MemoryRegion>,
}

/// The `/chosen` contract.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chosen {
    /// `stdout-path`, for example `serial10:115200n8`.
    pub stdout_path: Option<String>,
    /// `bootargs`, if the firmware supplied one.
    pub bootargs: Option<String>,
}

/// Where the selected UART lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UartSource {
    /// A BCM2712 peripheral (the `/soc` node).
    Soc,
    /// An RP1 peripheral behind the PCIe window.
    Rp1,
    /// A node outside the two known buses.
    Other,
}

/// The selected debug UART.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uart {
    /// Full device-tree path.
    pub node: String,
    /// CPU physical base.
    pub base: u64,
    /// Register-window length.
    pub size: u64,
    /// `clock-frequency` when the node supplies it; otherwise `None` (UNVERIFIED).
    pub clock_hz: Option<u32>,
    /// GIC SPI number when the node's `interrupts` is a three-cell GIC specifier.
    pub interrupt: Option<u32>,
    /// Which bus the UART is on.
    pub source: UartSource,
}

/// The GIC-400 register blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gic {
    /// Distributor (`GICD`).
    pub distributor: MemoryRegion,
    /// CPU interface (`GICC`).
    pub cpu_interface: MemoryRegion,
    /// Virtual interface (`GICH`), when the tree supplies it.
    pub virtual_interface: Option<MemoryRegion>,
    /// Virtual CPU interface (`GICV`), when the tree supplies it.
    pub virtual_cpu: Option<MemoryRegion>,
}

/// The BCM2835 system timer (the per-SoC MMIO timer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemTimer {
    /// CPU physical base.
    pub base: u64,
    /// Register-window length.
    pub size: u64,
    /// `clock-frequency` in Hz.
    pub clock_hz: u32,
}

/// The generic ARM timer and the optional BCM2835 system timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer {
    /// The non-secure physical PPI number from the `arm,armv8-timer` node (14 on the Pi 5).
    pub generic_ppi: u32,
    /// The kernel's normalised PPI number (`generic_ppi + 16`, 30 on the Pi 5), matching
    /// `raven-bootinfo-fdt`'s platform convention.
    pub kernel_ppi: u32,
    /// The BCM2835 system timer, when present.
    pub system: Option<SystemTimer>,
}

/// One PCIe `ranges` window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcieRange {
    /// PCI address-space flags (the first child cell).
    pub flags: u32,
    /// PCI address (the two remaining child cells).
    pub pci: u64,
    /// CPU physical address.
    pub cpu: u64,
    /// Window length.
    pub size: u64,
}

/// The RP1 I/O controller behind the PCIe root complex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rp1 {
    /// Full device-tree path.
    pub node: String,
    /// RP1-local base address of the first `ranges` window.
    pub child_base: u64,
    /// PCI address-space flags of the window.
    pub pci_flags: u32,
    /// PCI base address of the window.
    pub pci_address: u64,
    /// CPU physical base of the window.
    pub cpu_base: u64,
    /// Window length.
    pub size: u64,
}

impl Rp1 {
    /// Translate an RP1-local address through the window, or `None` when it is outside.
    #[must_use]
    pub fn translate(&self, address: u64) -> Option<u64> {
        let offset = address.checked_sub(self.child_base)?;
        if offset >= self.size {
            return None;
        }
        self.cpu_base.checked_add(offset)
    }
}

/// The BCM2712 PCIe root complex and its RP1 child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcie {
    /// Full device-tree path of the controller.
    pub node: String,
    /// Controller registers.
    pub controller: MemoryRegion,
    /// PCIe `ranges` windows.
    pub ranges: Vec<PcieRange>,
    /// The RP1 controller, when this tree has one.
    pub rp1: Option<Rp1>,
}

/// The parsed Pi 5 platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pi5 {
    /// `model` from the root.
    pub model: String,
    /// `compatible` from the root.
    pub compatible: Vec<String>,
    /// `/chosen`.
    pub chosen: Chosen,
    /// Memory and reserved memory.
    pub memory: MemoryMap,
    /// The selected debug UART.
    pub uart: Uart,
    /// The GIC-400.
    pub gic: Gic,
    /// The generic and system timers.
    pub timer: Timer,
    /// The PCIe root complex, when the tree has one.
    pub pcie: Option<Pcie>,
}

impl Pi5 {
    /// True when the root identifies a Raspberry Pi 5.
    #[must_use]
    pub fn is_raspberry_pi5(&self) -> bool {
        self.model.contains("Raspberry Pi 5")
            || self
                .compatible
                .iter()
                .any(|name| name == "raspberrypi,5-model-b")
    }
}

/// Why a Pi 5 tree could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The blob is not a supported flattened device tree.
    Fdt(FdtError),
    /// The FDT has no root node.
    MissingRoot,
    /// The root has no `/memory` node.
    MissingMemory,
    /// A memory or reserved-memory property is malformed, empty or overflowing.
    BadMemory(&'static str),
    /// Two `/memory` regions overlap.
    OverlappingMemory {
        /// The lower region.
        first: MemoryRegion,
        /// The region that starts before `first` ends.
        second: MemoryRegion,
    },
    /// Two reserved-memory regions overlap.
    OverlappingReserved {
        /// The lower region.
        first: MemoryRegion,
        /// The region that starts before `first` ends.
        second: MemoryRegion,
    },
    /// No PL011 node was found and no `stdout-path` resolved to one.
    MissingUart,
    /// A UART node is malformed.
    BadUart(&'static str),
    /// A UART address is outside every known translation window.
    UnmappedUart,
    /// No GIC-400/cortex-a15 GIC was found.
    MissingGic,
    /// A GIC node is malformed or has too few blocks.
    BadGic(&'static str),
    /// No generic ARM timer was found.
    MissingTimer,
    /// A timer node is malformed.
    BadTimer(&'static str),
    /// A PCIe node is malformed.
    BadPcie(&'static str),
    /// The RP1 node or its `ranges` are malformed.
    BadRp1(&'static str),
    /// The RP1 window does not fit any PCIe `ranges` entry.
    UnmappedRp1,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fdt(error) => write!(f, "device tree: {error}"),
            Self::MissingRoot => f.write_str("device tree has no root node"),
            Self::MissingMemory => f.write_str("device tree has no /memory node"),
            Self::BadMemory(reason) => write!(f, "bad memory node: {reason}"),
            Self::OverlappingMemory { first, second } => write!(
                f,
                "memory regions overlap: {:#x}+{:#x} and {:#x}+{:#x}",
                first.base, first.size, second.base, second.size
            ),
            Self::OverlappingReserved { first, second } => write!(
                f,
                "reserved-memory regions overlap: {:#x}+{:#x} and {:#x}+{:#x}",
                first.base, first.size, second.base, second.size
            ),
            Self::MissingUart => f.write_str("no PL011 UART node"),
            Self::BadUart(reason) => write!(f, "bad UART node: {reason}"),
            Self::UnmappedUart => f.write_str("UART address is outside every translation window"),
            Self::MissingGic => f.write_str("no GIC-400 node"),
            Self::BadGic(reason) => write!(f, "bad GIC node: {reason}"),
            Self::MissingTimer => f.write_str("no generic ARM timer node"),
            Self::BadTimer(reason) => write!(f, "bad timer node: {reason}"),
            Self::BadPcie(reason) => write!(f, "bad PCIe node: {reason}"),
            Self::BadRp1(reason) => write!(f, "bad RP1 node: {reason}"),
            Self::UnmappedRp1 => f.write_str("RP1 window does not fit the PCIe ranges"),
        }
    }
}

impl std::error::Error for Error {}

impl From<FdtError> for Error {
    fn from(error: FdtError) -> Self {
        Self::Fdt(error)
    }
}

/// Parse a Pi 5 firmware device tree.
///
/// # Errors
///
/// Returns [`Error`] for a malformed blob, a missing required node, overlapping memory, malformed
/// cell counts or an address that is outside every declared bus window.
pub fn parse(data: &[u8]) -> Result<Pi5, Error> {
    let fdt = Fdt::parse(data)?;
    let root = fdt.root().ok_or(Error::MissingRoot)?;
    let model = root.property_str("model").unwrap_or("").to_string();
    let compatible = strings(&root, "compatible");
    let memory = parse_memory(&root)?;
    let pcie = parse_pcie(&fdt)?;
    let gic = parse_gic(&fdt)?;
    let timer = parse_timer(&fdt)?;
    let chosen = parse_chosen(&fdt);
    let uart = parse_uart(&fdt, pcie.as_ref())?;
    Ok(Pi5 {
        model,
        compatible,
        chosen,
        memory,
        uart,
        gic,
        timer,
        pcie,
    })
}

fn parse_memory(root: &Node<'_>) -> Result<MemoryMap, Error> {
    if root.address_cells() > 2 || root.size_cells() > 2 {
        return Err(Error::BadMemory("unsupported root cell counts"));
    }
    let mut regions = Vec::new();
    for child in root.children() {
        let is_memory = child.property_str("device_type") == Some("memory")
            || child.name().starts_with("memory");
        if !is_memory {
            continue;
        }
        let reg = child
            .reg_checked(root.address_cells(), root.size_cells())
            .map_err(|_| Error::BadMemory("malformed memory reg"))?;
        for (base, size) in reg {
            regions.push(
                region(base, size).ok_or(Error::BadMemory("empty or overflowing memory region"))?,
            );
        }
    }
    if regions.is_empty() {
        return Err(Error::MissingMemory);
    }
    check_overlaps(&mut regions, false)?;
    let reserved = parse_reserved(root)?;
    Ok(MemoryMap { regions, reserved })
}

fn parse_reserved(root: &Node<'_>) -> Result<Vec<MemoryRegion>, Error> {
    let Some(reserved) = root
        .children()
        .find(|node| node.name() == "reserved-memory")
    else {
        return Ok(Vec::new());
    };
    let address_cells = reserved.address_cells();
    let size_cells = reserved.size_cells();
    if address_cells > 2 || size_cells > 2 {
        return Err(Error::BadMemory("unsupported reserved-memory cell counts"));
    }
    let mut regions = Vec::new();
    for child in reserved.children() {
        if !enabled(&child) || child.property("reg").is_none() {
            continue;
        }
        let reg = child
            .reg_checked(address_cells, size_cells)
            .map_err(|_| Error::BadMemory("malformed reserved-memory reg"))?;
        for (base, size) in reg {
            if let Some(region) = region(base, size) {
                regions.push(region);
            }
        }
    }
    check_overlaps(&mut regions, true)?;
    Ok(regions)
}

fn check_overlaps(regions: &mut [MemoryRegion], reserved: bool) -> Result<(), Error> {
    regions.sort_by_key(|region| region.base);
    for index in 1..regions.len() {
        let Some(previous) = regions.get(index - 1).copied() else {
            continue;
        };
        let Some(current) = regions.get(index).copied() else {
            continue;
        };
        if previous.end().is_some_and(|end| end > current.base) {
            return Err(if reserved {
                Error::OverlappingReserved {
                    first: previous,
                    second: current,
                }
            } else {
                Error::OverlappingMemory {
                    first: previous,
                    second: current,
                }
            });
        }
    }
    Ok(())
}

fn parse_gic(fdt: &Fdt<'_>) -> Result<Gic, Error> {
    let node = find_compatible_enabled(fdt, &["arm,gic-400", "arm,cortex-a15-gic"])
        .ok_or(Error::MissingGic)?;
    let parent = node.parent().ok_or(Error::BadGic("GIC has no parent"))?;
    let reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(|_| Error::BadGic("malformed GIC reg"))?;
    let mut regions = Vec::new();
    for (base, size) in reg {
        let cpu = raven_fdt_lite::translate_address(&node, base)
            .ok_or(Error::BadGic("GIC reg is outside the parent ranges"))?;
        regions.push(region(cpu, size).ok_or(Error::BadGic("empty GIC region"))?);
    }
    let distributor = regions
        .first()
        .copied()
        .ok_or(Error::BadGic("GIC has no distributor"))?;
    let cpu_interface = regions
        .get(1)
        .copied()
        .ok_or(Error::BadGic("GIC has no CPU interface"))?;
    if distributor.size < 0x1000 || cpu_interface.size < 0x2000 {
        return Err(Error::BadGic("GIC region is too small"));
    }
    Ok(Gic {
        distributor,
        cpu_interface,
        virtual_interface: regions.get(2).copied(),
        virtual_cpu: regions.get(3).copied(),
    })
}

fn parse_timer(fdt: &Fdt<'_>) -> Result<Timer, Error> {
    let node = find_compatible_enabled(fdt, &["arm,armv8-timer", "arm,armv7-timer"])
        .ok_or(Error::MissingTimer)?;
    let mut cells = node.cells("interrupts", 4);
    let mut generic_ppi = None;
    let mut index = 0usize;
    while let (Some(kind), Some(number)) = (cells.next(), cells.next()) {
        let _flags = cells
            .next()
            .ok_or(Error::BadTimer("truncated timer interrupts"))?;
        if index == 1 && kind == 1 {
            generic_ppi =
                Some(u32::try_from(number).map_err(|_| Error::BadTimer("timer PPI is too large"))?);
        }
        index = index.saturating_add(1);
        if index >= 4 {
            break;
        }
    }
    let generic_ppi = generic_ppi.ok_or(Error::BadTimer("no non-secure physical timer PPI"))?;
    let kernel_ppi = generic_ppi
        .checked_add(16)
        .ok_or(Error::BadTimer("timer PPI overflow"))?;
    let system = find_compatible_enabled(fdt, &["brcm,bcm2835-system-timer"])
        .map(parse_system_timer)
        .transpose()?;
    Ok(Timer {
        generic_ppi,
        kernel_ppi,
        system,
    })
}

fn parse_system_timer(node: Node<'_>) -> Result<SystemTimer, Error> {
    let parent = node
        .parent()
        .ok_or(Error::BadTimer("system timer has no parent"))?;
    let mut reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(|_| Error::BadTimer("malformed system timer reg"))?;
    let (base, size) = reg
        .next()
        .ok_or(Error::BadTimer("system timer has no reg"))?;
    let cpu = raven_fdt_lite::translate_address(&node, base)
        .ok_or(Error::BadTimer("system timer reg is unmapped"))?;
    let region = region(cpu, size).ok_or(Error::BadTimer("empty system timer region"))?;
    let clock_hz = node
        .u32_property("clock-frequency")
        .ok_or(Error::BadTimer("system timer has no clock-frequency"))?;
    Ok(SystemTimer {
        base: region.base,
        size: region.size,
        clock_hz,
    })
}

fn parse_pcie(fdt: &Fdt<'_>) -> Result<Option<Pcie>, Error> {
    let root = fdt.root().ok_or(Error::MissingRoot)?;
    let Some(axi) = root.children().find(|node| node.name().starts_with("axi")) else {
        return Ok(None);
    };
    for child in axi.children() {
        if !child.compatible_with("brcm,bcm2712-pcie") || !enabled(&child) {
            continue;
        }
        let parent = child.parent().ok_or(Error::BadPcie("PCIe has no parent"))?;
        let mut reg = child
            .reg_checked(parent.address_cells(), parent.size_cells())
            .map_err(|_| Error::BadPcie("malformed PCIe reg"))?;
        let (base, size) = reg.next().ok_or(Error::BadPcie("PCIe has no reg"))?;
        let cpu = raven_fdt_lite::translate_address(&child, base)
            .ok_or(Error::BadPcie("PCIe reg is unmapped"))?;
        let controller = region(cpu, size).ok_or(Error::BadPcie("empty PCIe region"))?;
        let ranges = parse_pcie_ranges(&child)?;
        let rp1 = child
            .children()
            .find(|node| node.name() == "rp1" || node.name().starts_with("rp1@"));
        let rp1 = rp1.map(|node| parse_rp1(&node, &ranges)).transpose()?;
        return Ok(Some(Pcie {
            node: node_path(&child),
            controller,
            ranges,
            rp1,
        }));
    }
    Ok(None)
}

fn parse_pcie_ranges(node: &Node<'_>) -> Result<Vec<PcieRange>, Error> {
    let Some(value) = node.property("ranges") else {
        return Ok(Vec::new());
    };
    if !value.len().is_multiple_of(7 * 4) {
        return Err(Error::BadPcie("malformed PCIe ranges"));
    }
    let mut cells = node.cells("ranges", 4);
    let mut ranges = Vec::new();
    while let Some(flags) = cells.next() {
        let pci_high = cells
            .next()
            .ok_or(Error::BadPcie("truncated PCIe ranges"))?;
        let pci_low = cells
            .next()
            .ok_or(Error::BadPcie("truncated PCIe ranges"))?;
        let cpu_high = cells
            .next()
            .ok_or(Error::BadPcie("truncated PCIe ranges"))?;
        let cpu_low = cells
            .next()
            .ok_or(Error::BadPcie("truncated PCIe ranges"))?;
        let size_high = cells
            .next()
            .ok_or(Error::BadPcie("truncated PCIe ranges"))?;
        let size_low = cells
            .next()
            .ok_or(Error::BadPcie("truncated PCIe ranges"))?;
        let size = (size_high << 32) | size_low;
        if size == 0 {
            continue;
        }
        ranges.push(PcieRange {
            flags: u32::try_from(flags)
                .map_err(|_| Error::BadPcie("PCIe range flags are too large"))?,
            pci: (pci_high << 32) | pci_low,
            cpu: (cpu_high << 32) | cpu_low,
            size,
        });
    }
    Ok(ranges)
}

fn parse_rp1(node: &Node<'_>, pcie_ranges: &[PcieRange]) -> Result<Rp1, Error> {
    let value = node
        .property("ranges")
        .ok_or(Error::BadRp1("RP1 has no ranges"))?;
    if !value.len().is_multiple_of(7 * 4) {
        return Err(Error::BadRp1("malformed RP1 ranges"));
    }
    let mut cells = node.cells("ranges", 4);
    while let Some(child_high) = cells.next() {
        let child_low = cells.next().ok_or(Error::BadRp1("truncated RP1 ranges"))?;
        let flags = cells.next().ok_or(Error::BadRp1("truncated RP1 ranges"))?;
        let pci_high = cells.next().ok_or(Error::BadRp1("truncated RP1 ranges"))?;
        let pci_low = cells.next().ok_or(Error::BadRp1("truncated RP1 ranges"))?;
        let size_high = cells.next().ok_or(Error::BadRp1("truncated RP1 ranges"))?;
        let size_low = cells.next().ok_or(Error::BadRp1("truncated RP1 ranges"))?;
        let child_base = (child_high << 32) | child_low;
        let pci_address = (pci_high << 32) | pci_low;
        let size = (size_high << 32) | size_low;
        if size == 0 {
            continue;
        }
        let flags =
            u32::try_from(flags).map_err(|_| Error::BadRp1("RP1 range flags are too large"))?;
        let pci_end = pci_address
            .checked_add(size)
            .ok_or(Error::BadRp1("RP1 range overflow"))?;
        let Some(range) = pcie_ranges.iter().find(|range| {
            range.flags == flags
                && pci_address >= range.pci
                && range
                    .pci
                    .checked_add(range.size)
                    .is_some_and(|end| pci_end <= end)
        }) else {
            continue;
        };
        let offset = pci_address
            .checked_sub(range.pci)
            .ok_or(Error::BadRp1("RP1 range offset overflow"))?;
        let cpu_base = range
            .cpu
            .checked_add(offset)
            .ok_or(Error::BadRp1("RP1 CPU base overflow"))?;
        return Ok(Rp1 {
            node: node_path(node),
            child_base,
            pci_flags: flags,
            pci_address,
            cpu_base,
            size,
        });
    }
    Err(Error::UnmappedRp1)
}

fn parse_uart(fdt: &Fdt<'_>, pcie: Option<&Pcie>) -> Result<Uart, Error> {
    if let Some(stdout) = parse_chosen(fdt).stdout_path {
        let alias = stdout
            .split_once(':')
            .map_or(stdout.as_str(), |(alias, _)| alias);
        if let Some(node) = resolve_alias(fdt, alias)
            && is_pl011(&node)
        {
            return uart_from_node(&node, pcie);
        }
    }
    for prefer_soc in [true, false] {
        if let Some(node) = find_pl011(fdt, prefer_soc) {
            return uart_from_node(&node, pcie);
        }
    }
    Err(Error::MissingUart)
}

fn parse_chosen(fdt: &Fdt<'_>) -> Chosen {
    let Some(chosen) = fdt.find_node("/chosen") else {
        return Chosen::default();
    };
    Chosen {
        stdout_path: chosen.property_str("stdout-path").map(str::to_string),
        bootargs: chosen.property_str("bootargs").map(str::to_string),
    }
}

fn resolve_alias<'a>(fdt: &Fdt<'a>, alias: &str) -> Option<Node<'a>> {
    let root = fdt.root()?;
    let aliases = root.children().find(|node| node.name() == "aliases")?;
    let path = aliases.property_str(alias)?;
    fdt.find_node(path)
}

fn uart_from_node(node: &Node<'_>, pcie: Option<&Pcie>) -> Result<Uart, Error> {
    let path = node_path(node);
    let source = if path.starts_with("/soc") {
        UartSource::Soc
    } else if path.contains("/rp1") {
        UartSource::Rp1
    } else {
        UartSource::Other
    };
    let parent = node.parent().ok_or(Error::BadUart("UART has no parent"))?;
    let mut reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(|_| Error::BadUart("malformed UART reg"))?;
    let (base, size) = reg.next().ok_or(Error::BadUart("UART has no reg"))?;
    let cpu_base = match source {
        UartSource::Rp1 => {
            let rp1 = pcie
                .and_then(|pcie| pcie.rp1.as_ref())
                .ok_or(Error::UnmappedUart)?;
            rp1.translate(base).ok_or(Error::UnmappedUart)?
        }
        UartSource::Soc | UartSource::Other => {
            raven_fdt_lite::translate_address(node, base).ok_or(Error::UnmappedUart)?
        }
    };
    let region = region(cpu_base, size).ok_or(Error::BadUart("empty UART region"))?;
    Ok(Uart {
        node: path,
        base: region.base,
        size: region.size,
        clock_hz: node.u32_property("clock-frequency"),
        interrupt: parse_spi(node),
        source,
    })
}

fn parse_spi(node: &Node<'_>) -> Option<u32> {
    let mut cells = node.cells("interrupts", 4);
    let kind = cells.next()?;
    let number = cells.next()?;
    let _flags = cells.next()?;
    if kind != 0 {
        return None;
    }
    u32::try_from(number).ok()
}

fn is_pl011(node: &Node<'_>) -> bool {
    node.compatible_with("arm,pl011") || node.compatible_with("arm,pl011-axi")
}

fn find_pl011<'a>(fdt: &Fdt<'a>, prefer_soc: bool) -> Option<Node<'a>> {
    let root = fdt.root()?;
    let predicate = |node: &Node<'a>| {
        enabled(node) && is_pl011(node) && (!prefer_soc || node_path(node).starts_with("/soc"))
    };
    walk_find(root, &predicate)
}

fn find_compatible_enabled<'a>(fdt: &Fdt<'a>, names: &[&str]) -> Option<Node<'a>> {
    let root = fdt.root()?;
    let predicate =
        |node: &Node<'a>| names.iter().any(|name| node.compatible_with(name)) && enabled(node);
    walk_find(root, &predicate)
}

fn walk_find<'a>(node: Node<'a>, predicate: &dyn Fn(&Node<'a>) -> bool) -> Option<Node<'a>> {
    if predicate(&node) {
        return Some(node);
    }
    for child in node.children() {
        if let Some(found) = walk_find(child, predicate) {
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

fn node_path(node: &Node<'_>) -> String {
    let mut parts = Vec::new();
    let mut current = Some(*node);
    while let Some(item) = current {
        if !item.name().is_empty() {
            parts.push(item.name().to_string());
        }
        current = item.parent();
    }
    parts.reverse();
    let mut path = String::from("/");
    path.push_str(&parts.join("/"));
    path
}

fn strings(node: &Node<'_>, name: &str) -> Vec<String> {
    node.property(name)
        .map(|value| {
            value
                .split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn region(base: u64, size: u64) -> Option<MemoryRegion> {
    if size == 0 {
        return None;
    }
    base.checked_add(size)?;
    Some(MemoryRegion { base, size })
}

#[cfg(test)]
#[path = "parse_ut.rs"]
mod tests;
