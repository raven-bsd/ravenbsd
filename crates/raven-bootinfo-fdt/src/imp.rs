// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FDT to `BootInfo` and platform description.
//!
//! The builder never allocates: the caller supplies the memory-map and command-line buffers and
//! their physical addresses (the Pi 4 entry stub keeps both in its `.bss`). Every FDT access is
//! bounds-checked by [`crate::fdt`], and every arithmetic step is checked; malformed firmware data
//! becomes an [`Error`], never a panic.

use core::fmt;

use raven_bootinfo::validate_memory_map;
use raven_bootinfo::{
    BOOTINFO_MAGIC, BOOTINFO_VERSION, BootInfo, MemoryKind, MemoryRegion, PhysRange, arch,
};

use crate::fdt::{Cells, Fdt, Node};

/// Maximum memory-map entries a caller must provide.
pub const MAX_REGIONS: usize = 64;
/// Maximum command-line bytes a caller must provide.
pub const CMDLINE_MAX: usize = 1024;
/// Intermediate interval slots used while subtracting reservations.
const MAX_INTERVALS: usize = 128;
/// Longest board model kept from the FDT.
const MODEL_MAX: usize = 64;
/// Page size used by the memory map.
const PAGE_SIZE: u64 = 4096;
/// Number of `ranges` entries parsed for a bus.
const MAX_RANGES: usize = 8;

/// Caller-supplied addresses and kernel parameters.
#[derive(Clone, Copy, Debug, Default)]
pub struct Params {
    /// Virtual address of physical address zero.
    pub hhdm_offset: u64,
    /// Physical address of the kernel's `.text` (the higher-half image).
    pub kernel_phys_base: u64,
    /// Virtual address of the kernel's `.text`.
    pub kernel_virt_base: u64,
    /// Size of the higher-half kernel image.
    pub kernel_size: u64,
    /// Physical start of the whole loaded flat image (including the low entry stub).
    pub image_phys_base: u64,
    /// Size of the whole loaded flat image.
    pub image_size: u64,
    /// Physical address of the DTB.
    pub dtb_phys: u64,
    /// DTB length.
    pub dtb_len: u64,
    /// Fallback initfs range when `/chosen` has no initrd.
    pub initfs: PhysRange,
    /// `MPIDR_EL1` of the boot CPU.
    pub boot_cpu_id: u64,
    /// `CNTFRQ_EL0`.
    pub counter_freq: u64,
    /// Raw `CNTPCT_EL0` at entry.
    pub entry_counter: u64,
    /// Physical address of the caller's memory-map buffer.
    pub memory_map_phys: u64,
    /// Physical address of the caller's command-line buffer.
    pub cmdline_phys: u64,
}

/// Why a `BootInfo` could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The FDT has no root or no usable `/memory` node.
    NoMemory,
    /// The caller's memory-map buffer is too small.
    TooManyRegions,
    /// The bootargs string does not fit the caller's buffer.
    CmdlineTooLong,
    /// A property or `reg` value is malformed.
    BadProperty,
    /// An address overflowed while aligning or subtracting.
    BadAddress,
    /// The caller passed an empty output buffer.
    EmptyBuffer,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoMemory => f.write_str("device tree has no usable memory"),
            Error::TooManyRegions => f.write_str("memory map buffer is too small"),
            Error::CmdlineTooLong => f.write_str("bootargs do not fit the buffer"),
            Error::BadProperty => f.write_str("malformed device-tree property"),
            Error::BadAddress => f.write_str("address overflow"),
            Error::EmptyBuffer => f.write_str("empty boot-info buffer"),
        }
    }
}

/// A platform description parsed from the FDT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform {
    /// GIC distributor physical base.
    pub gicd: u64,
    /// GIC CPU-interface physical base.
    pub gicc: u64,
    /// Non-secure physical timer PPI number.
    pub timer_ppi: u32,
    /// PL011 physical base.
    pub uart: u64,
    /// PL011 input clock in Hz.
    pub uart_clock_hz: u32,
    /// True when at least one value came from the FDT.
    pub from_dtb: bool,
    model: Model,
}

/// A fixed-size board model string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model {
    bytes: [u8; MODEL_MAX],
    len: u8,
}

impl Model {
    /// An empty model.
    pub const fn empty() -> Self {
        Self {
            bytes: [0; MODEL_MAX],
            len: 0,
        }
    }

    /// Copy `text`, truncating at [`MODEL_MAX`] bytes.
    pub fn set(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let length = bytes.len().min(MODEL_MAX);
        if let Some(slot) = self.bytes.get_mut(..length) {
            slot.copy_from_slice(bytes.get(..length).unwrap_or(&[]));
        }
        self.len = u8::try_from(length).unwrap_or(u8::MAX);
    }

    /// The model as UTF-8, or an empty string when absent or truncated mid-codepoint.
    pub fn as_str(&self) -> &str {
        let length = usize::from(self.len).min(MODEL_MAX);
        core::str::from_utf8(self.bytes.get(..length).unwrap_or(&[])).unwrap_or("")
    }

    /// True when no model was recorded.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::empty()
    }
}

impl Platform {
    /// The QEMU `virt` defaults (also the UEFI path's fallback).
    pub const fn qemu_virt() -> Self {
        Self {
            gicd: 0x0800_0000,
            gicc: 0x0801_0000,
            timer_ppi: 30,
            uart: 0x0900_0000,
            uart_clock_hz: 24_000_000,
            from_dtb: false,
            model: Model::empty(),
        }
    }

    /// The board model, if the FDT had one.
    pub fn board_model(&self) -> Option<&str> {
        (!self.model.is_empty()).then(|| self.model.as_str())
    }

    /// True when the model looks like a Raspberry Pi.
    pub fn is_raspberry_pi(&self) -> bool {
        self.model.as_str().contains("Raspberry Pi")
    }
}

impl Default for Platform {
    fn default() -> Self {
        Self::qemu_virt()
    }
}

/// Parse the platform devices from a validated FDT.
///
/// Missing or malformed nodes leave their values at the QEMU defaults. The function never panics
/// and never trusts the property lengths beyond the FDT reader's checks.
pub fn platform(fdt: &Fdt<'_>) -> Platform {
    let mut result = Platform::qemu_virt();
    if let Some(root) = fdt.root()
        && let Some(model) = root.property_str("model")
    {
        result.model.set(model);
    }

    let mut entries = [(0u64, 0u64); 4];
    if let Some(node) = fdt.find_compatible(&["arm,pl011", "arm,pl011-axi"])
        && let Some(count) = reg_entries(fdt, &node, &mut entries)
        && count > 0
        && entries[0].1 != 0
    {
        result.uart = entries[0].0;
        result.from_dtb = true;
    }
    if result.is_raspberry_pi() || result.uart >= 0xFE00_0000 {
        result.uart_clock_hz = 48_000_000;
    }

    if let Some(node) = fdt.find_compatible(&["arm,cortex-a15-gic", "arm,gic-400"])
        && reg_entries(fdt, &node, &mut entries).is_some()
    {
        if let Some((base, size)) = entries.first().copied()
            && size != 0
        {
            result.gicd = base;
            result.from_dtb = true;
        }
        if let Some((base, size)) = entries.get(1).copied()
            && size != 0
        {
            result.gicc = base;
            result.from_dtb = true;
        }
    }

    if let Some(timer) = fdt.find_compatible(&["arm,armv8-timer", "arm,armv7-timer"]) {
        let mut cells = timer.cells("interrupts", 4);
        let mut index = 0usize;
        while index < 6 {
            let Some(kind) = cells.next() else { break };
            let Some(number) = cells.next() else { break };
            let Some(_flags) = cells.next() else { break };
            if index == 1 && kind == 1 && number < 16 {
                result.timer_ppi = u32::try_from(number).unwrap_or(0).saturating_add(16);
                result.from_dtb = true;
            }
            index = index.saturating_add(1);
        }
    }

    result
}

/// Build a `BootInfo` and fill the caller's memory-map and command-line buffers.
///
/// `params.memory_map_phys` and `params.cmdline_phys` are recorded in the returned `BootInfo`; the
/// buffers themselves are filled here. Both are page-aligned in the caller's address space.
pub fn build_into(
    fdt: &Fdt<'_>,
    params: &Params,
    memory_map: &mut [MemoryRegion],
    cmdline: &mut [u8],
) -> Result<BootInfo, Error> {
    if memory_map.is_empty() || cmdline.is_empty() {
        return Err(Error::EmptyBuffer);
    }
    let root = fdt.root().ok_or(Error::BadProperty)?;
    let (root_address_cells, root_size_cells) = node_cells(&root);

    let mut intervals = [Interval::EMPTY; MAX_INTERVALS];
    let mut count = 0usize;
    let mut memory_found = false;
    for child in root.children() {
        let is_memory = child.property_str("device_type") == Some("memory")
            || child.name().starts_with("memory");
        if !is_memory {
            continue;
        }
        let Ok(regs) = child.reg_checked(root_address_cells, root_size_cells) else {
            continue;
        };
        for (base, size) in regs {
            if size == 0 {
                continue;
            }
            let (start, end) = page_range(base, size)?;
            push_interval(&mut intervals, &mut count, start, end, MemoryKind::Usable)?;
            memory_found = true;
        }
    }
    if !memory_found {
        return Err(Error::NoMemory);
    }

    // Every reservation is carved out of the usable ranges first.
    for (base, size) in fdt.reservations() {
        subtract(&mut intervals, &mut count, base, size)?;
    }
    if let Some(reserved) = fdt.find_node("/reserved-memory") {
        let (address_cells, size_cells) = node_cells(&reserved);
        for child in reserved.children() {
            let Ok(regs) = child.reg_checked(address_cells, size_cells) else {
                continue;
            };
            for (base, size) in regs {
                subtract(&mut intervals, &mut count, base, size)?;
            }
        }
    }

    let initfs = chosen_initfs(fdt).unwrap_or(params.initfs);
    let reserved_ranges = [
        (
            params.image_phys_base,
            params.image_size,
            MemoryKind::KernelAndModules,
        ),
        (params.dtb_phys, params.dtb_len, MemoryKind::Reserved),
        (initfs.base, initfs.len, MemoryKind::KernelAndModules),
    ];
    for (base, size, _) in reserved_ranges {
        subtract(&mut intervals, &mut count, base, size)?;
    }

    for (base, size, kind) in reserved_ranges {
        if size != 0 {
            add_interval(&mut intervals, &mut count, base, size, kind)?;
        }
    }
    add_device_ranges(
        fdt,
        &["arm,pl011", "arm,pl011-axi"],
        &mut intervals,
        &mut count,
    )?;
    add_device_ranges(
        fdt,
        &["arm,cortex-a15-gic", "arm,gic-400"],
        &mut intervals,
        &mut count,
    )?;

    sort_and_merge(&mut intervals, count);
    let mut final_count = 0usize;
    for interval in intervals.iter().take(MAX_INTERVALS) {
        if interval.is_empty() {
            continue;
        }
        if final_count >= memory_map.len() {
            return Err(Error::TooManyRegions);
        }
        let Some(slot) = memory_map.get_mut(final_count) else {
            return Err(Error::TooManyRegions);
        };
        *slot = MemoryRegion {
            base: interval.start,
            len: interval.end.saturating_sub(interval.start),
            kind: interval.kind as u32,
            reserved: 0,
        };
        final_count = final_count.saturating_add(1);
    }
    if final_count == 0 {
        return Err(Error::NoMemory);
    }
    let map = memory_map.get(..final_count).ok_or(Error::TooManyRegions)?;
    if validate_memory_map(map).is_err() {
        return Err(Error::BadProperty);
    }

    let cmdline_length = copy_cmdline(fdt, cmdline)?;
    let mut info = BootInfo::zeroed();
    info.magic = BOOTINFO_MAGIC;
    info.version = BOOTINFO_VERSION;
    info.size = core::mem::size_of::<BootInfo>() as u32;
    info.arch = arch::AARCH64;
    info.hhdm_offset = params.hhdm_offset;
    info.kernel_phys_base = params.kernel_phys_base;
    info.kernel_virt_base = params.kernel_virt_base;
    info.kernel_size = params.kernel_size;
    info.memory_map_base = params.memory_map_phys;
    info.memory_map_len = final_count as u64;
    info.memory_map_entry_size = core::mem::size_of::<MemoryRegion>() as u64;
    info.initfs = if initfs.len != 0 {
        initfs
    } else {
        PhysRange { base: 0, len: 0 }
    };
    info.cmdline = if cmdline_length != 0 {
        PhysRange {
            base: params.cmdline_phys,
            len: cmdline_length as u64,
        }
    } else {
        PhysRange { base: 0, len: 0 }
    };
    info.dtb = if params.dtb_phys != 0 && params.dtb_len != 0 {
        PhysRange {
            base: params.dtb_phys,
            len: params.dtb_len,
        }
    } else {
        PhysRange { base: 0, len: 0 }
    };
    info.boot_cpu_id = params.boot_cpu_id;
    info.entry_counter = params.entry_counter;
    info.counter_freq = params.counter_freq;
    Ok(info)
}

/// One interval while building the map.
#[derive(Clone, Copy, Debug)]
struct Interval {
    start: u64,
    end: u64,
    kind: MemoryKind,
}

impl Interval {
    const EMPTY: Self = Self {
        start: 0,
        end: 0,
        kind: MemoryKind::Usable,
    };

    const fn is_empty(self) -> bool {
        self.end <= self.start
    }
}

/// One `ranges` entry.
#[derive(Clone, Copy)]
struct RangeMap {
    child: u64,
    parent: u64,
    len: u64,
}

fn push_interval(
    intervals: &mut [Interval; MAX_INTERVALS],
    count: &mut usize,
    start: u64,
    end: u64,
    kind: MemoryKind,
) -> Result<(), Error> {
    if end <= start {
        return Ok(());
    }
    if *count >= MAX_INTERVALS {
        return Err(Error::TooManyRegions);
    }
    let Some(slot) = intervals.get_mut(*count) else {
        return Err(Error::TooManyRegions);
    };
    *slot = Interval { start, end, kind };
    *count = count.saturating_add(1);
    Ok(())
}

fn add_interval(
    intervals: &mut [Interval; MAX_INTERVALS],
    count: &mut usize,
    base: u64,
    size: u64,
    kind: MemoryKind,
) -> Result<(), Error> {
    let (start, end) = page_range(base, size)?;
    push_interval(intervals, count, start, end, kind)
}

fn add_device_ranges(
    fdt: &Fdt<'_>,
    names: &[&str],
    intervals: &mut [Interval; MAX_INTERVALS],
    count: &mut usize,
) -> Result<(), Error> {
    let Some(node) = fdt.find_compatible(names) else {
        return Ok(());
    };
    let mut entries = [(0u64, 0u64); 4];
    let Some(found) = reg_entries(fdt, &node, &mut entries) else {
        return Ok(());
    };
    for (base, size) in entries.iter().take(found.min(entries.len())) {
        if *size != 0 {
            add_interval(intervals, count, *base, *size, MemoryKind::Mmio)?;
        }
    }
    Ok(())
}

fn subtract(
    intervals: &mut [Interval; MAX_INTERVALS],
    count: &mut usize,
    base: u64,
    size: u64,
) -> Result<(), Error> {
    if size == 0 {
        return Ok(());
    }
    let (start, end) = page_range(base, size)?;
    let mut next = [Interval::EMPTY; MAX_INTERVALS];
    let mut next_count = 0usize;
    for interval in intervals.iter().take(*count) {
        if interval.is_empty() || interval.end <= start || interval.start >= end {
            if next_count >= MAX_INTERVALS {
                return Err(Error::TooManyRegions);
            }
            if let Some(slot) = next.get_mut(next_count) {
                *slot = *interval;
            }
            next_count = next_count.saturating_add(1);
            continue;
        }
        if interval.start < start {
            if next_count >= MAX_INTERVALS {
                return Err(Error::TooManyRegions);
            }
            if let Some(slot) = next.get_mut(next_count) {
                *slot = Interval {
                    end: start,
                    ..*interval
                };
            }
            next_count = next_count.saturating_add(1);
        }
        if interval.end > end {
            if next_count >= MAX_INTERVALS {
                return Err(Error::TooManyRegions);
            }
            if let Some(slot) = next.get_mut(next_count) {
                *slot = Interval {
                    start: end,
                    ..*interval
                };
            }
            next_count = next_count.saturating_add(1);
        }
    }
    intervals.copy_from_slice(&next);
    *count = next_count;
    Ok(())
}

fn sort_and_merge(intervals: &mut [Interval; MAX_INTERVALS], count: usize) {
    let Some(slice) = intervals.get_mut(..count) else {
        return;
    };
    slice.sort_unstable_by_key(|interval| interval.start);
    let mut write = 0usize;
    for read in 0..slice.len() {
        let mut current = slice[read];
        if current.is_empty() {
            continue;
        }
        if write > 0 {
            let Some(previous) = slice.get(write - 1).copied() else {
                continue;
            };
            if current.start < previous.end {
                if current.kind == previous.kind {
                    current = Interval {
                        start: previous.start,
                        end: current.end.max(previous.end),
                        kind: previous.kind,
                    };
                    write = write.saturating_sub(1);
                } else {
                    current.start = previous.end;
                }
            }
        }
        if current.is_empty() {
            continue;
        }
        if let Some(slot) = slice.get_mut(write) {
            *slot = current;
        }
        write = write.saturating_add(1);
    }
    for index in write..slice.len() {
        if let Some(slot) = slice.get_mut(index) {
            *slot = Interval::EMPTY;
        }
    }
}

fn page_range(base: u64, size: u64) -> Result<(u64, u64), Error> {
    let end = base.checked_add(size).ok_or(Error::BadAddress)?;
    let start = base & !(PAGE_SIZE - 1);
    let aligned_end = end
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
        .ok_or(Error::BadAddress)?;
    if aligned_end <= start {
        return Err(Error::BadAddress);
    }
    Ok((start, aligned_end))
}

fn chosen_initfs(fdt: &Fdt<'_>) -> Option<PhysRange> {
    let chosen = fdt.find_node("/chosen")?;
    let start = property_u64(&chosen, "linux,initrd-start")?;
    let end = property_u64(&chosen, "linux,initrd-end")?;
    (end > start).then_some(PhysRange {
        base: start,
        len: end - start,
    })
}

fn copy_cmdline(fdt: &Fdt<'_>, buffer: &mut [u8]) -> Result<usize, Error> {
    let Some(chosen) = fdt.find_node("/chosen") else {
        return Ok(0);
    };
    let Some(bytes) = chosen.property("bootargs") else {
        return Ok(0);
    };
    let mut length = 0usize;
    while length < bytes.len() {
        let Some(byte) = bytes.get(length).copied() else {
            break;
        };
        if byte == 0 {
            break;
        }
        length = length.saturating_add(1);
    }
    if length >= buffer.len() {
        return Err(Error::CmdlineTooLong);
    }
    if let Some(slot) = buffer.get_mut(..length)
        && let Some(source) = bytes.get(..length)
    {
        slot.copy_from_slice(source);
    }
    Ok(length)
}

fn property_u64(node: &Node<'_>, name: &str) -> Option<u64> {
    let value = node.property(name)?;
    if value.len() == 8 {
        read_be_u64(value, 0)
    } else if value.len() == 4 {
        read_be_u32(value, 0).map(u64::from)
    } else {
        None
    }
}

fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn read_be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes: [u8; 8] = data.get(offset..offset.checked_add(8)?)?.try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

fn node_cells(node: &Node<'_>) -> (u32, u32) {
    (
        node.u32_property("#address-cells").unwrap_or(2),
        node.u32_property("#size-cells").unwrap_or(1),
    )
}

fn node_parent<'a>(fdt: &Fdt<'a>, node: &Node<'a>) -> Option<Node<'a>> {
    if let Some(soc) = fdt.find_node("/soc")
        && soc.children().any(|child| {
            child.name() == node.name() && child.property("reg") == node.property("reg")
        })
    {
        return Some(soc);
    }
    fdt.root()
}

fn reg_entries(fdt: &Fdt<'_>, node: &Node<'_>, out: &mut [(u64, u64); 4]) -> Option<usize> {
    let parent = node_parent(fdt, node)?;
    let (address_cells, size_cells) = node_cells(&parent);
    let (parent_address_cells, _) = node_cells(&fdt.root()?);
    let ranges = parse_ranges(
        fdt,
        &parent,
        address_cells,
        size_cells,
        parent_address_cells,
    );
    let regs = node.reg_checked(address_cells, size_cells).ok()?;
    let mut count = 0usize;
    for (address, size) in regs {
        if count >= out.len() {
            break;
        }
        let translated = translate_ranges(&ranges, address, size);
        if let Some((base, length)) = translated
            && let Some(slot) = out.get_mut(count)
        {
            *slot = (base, length);
            count = count.saturating_add(1);
        }
    }
    Some(count)
}

fn parse_ranges(
    fdt: &Fdt<'_>,
    parent: &Node<'_>,
    child_address_cells: u32,
    child_size_cells: u32,
    parent_address_cells: u32,
) -> ([RangeMap; MAX_RANGES], usize) {
    let mut ranges = [RangeMap {
        child: 0,
        parent: 0,
        len: 0,
    }; MAX_RANGES];
    let mut count = 0usize;
    let _ = fdt;
    if parent.property("ranges").is_none() {
        return (ranges, 0);
    }
    let mut cells = parent.cells("ranges", 4);
    loop {
        if count >= MAX_RANGES {
            break;
        }
        let Some(child) = read_cells(&mut cells, child_address_cells) else {
            break;
        };
        let Some(parent_address) = read_cells(&mut cells, parent_address_cells) else {
            break;
        };
        let Some(length) = read_cells(&mut cells, child_size_cells) else {
            break;
        };
        if let Some(slot) = ranges.get_mut(count) {
            *slot = RangeMap {
                child,
                parent: parent_address,
                len: length,
            };
        }
        count = count.saturating_add(1);
    }
    (ranges, count)
}

fn translate_ranges(
    ranges: &([RangeMap; MAX_RANGES], usize),
    address: u64,
    size: u64,
) -> Option<(u64, u64)> {
    if ranges.1 == 0 {
        return Some((address, size));
    }
    for range in ranges.0.iter().take(ranges.1) {
        let end = range.child.checked_add(range.len)?;
        if address >= range.child && address < end {
            let offset = address.checked_sub(range.child)?;
            let base = range.parent.checked_add(offset)?;
            return Some((base, size));
        }
    }
    None
}

fn read_cells(cells: &mut Cells<'_>, count: u32) -> Option<u64> {
    let mut value = 0u64;
    let mut index = 0u32;
    while index < count {
        value = value.checked_shl(32)?.checked_add(cells.next()?)?;
        index = index.saturating_add(1);
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use raven_bootinfo::{BOOTINFO_MAGIC, BOOTINFO_VERSION, MemoryKind, arch, validate_memory_map};
    use raven_fdt_lite::{FDT_BEGIN_NODE, FDT_END, FDT_END_NODE, FDT_MAGIC, FDT_PROP};

    const HHDM: u64 = 0xFFFF_8000_0000_0000;
    const KERNEL: u64 = 0xFFFF_FFFF_8020_0000;

    struct Builder {
        structure: Vec<u8>,
        strings: Vec<u8>,
        reservations: Vec<(u64, u64)>,
    }

    impl Builder {
        fn new() -> Self {
            Self {
                structure: Vec::new(),
                strings: Vec::new(),
                reservations: Vec::new(),
            }
        }

        fn token(&mut self, token: u32) {
            self.structure.extend_from_slice(&token.to_be_bytes());
        }

        fn string_offset(&mut self, value: &str) -> u32 {
            let offset = self.strings.len() as u32;
            self.strings.extend_from_slice(value.as_bytes());
            self.strings.push(0);
            offset
        }

        fn begin(&mut self, name: &str) {
            self.token(FDT_BEGIN_NODE);
            self.structure.extend_from_slice(name.as_bytes());
            self.structure.push(0);
            while !self.structure.len().is_multiple_of(4) {
                self.structure.push(0);
            }
        }

        fn end(&mut self) {
            self.token(FDT_END_NODE);
        }

        fn prop(&mut self, name: &str, value: &[u8]) {
            let name_offset = self.string_offset(name);
            self.token(FDT_PROP);
            self.structure
                .extend_from_slice(&(value.len() as u32).to_be_bytes());
            self.structure.extend_from_slice(&name_offset.to_be_bytes());
            self.structure.extend_from_slice(value);
            while !self.structure.len().is_multiple_of(4) {
                self.structure.push(0);
            }
        }

        fn cells_prop(&mut self, name: &str, values: &[u32]) {
            let mut bytes = Vec::new();
            for value in values {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            self.prop(name, &bytes);
        }

        fn string_prop(&mut self, name: &str, value: &str) {
            let mut bytes = value.as_bytes().to_vec();
            bytes.push(0);
            self.prop(name, &bytes);
        }

        fn reservation(&mut self, address: u64, size: u64) {
            self.reservations.push((address, size));
        }

        fn finish(&mut self) -> Vec<u8> {
            self.token(FDT_END);
            let reservation_bytes = (self.reservations.len() + 1) * 16;
            let struct_offset = 40 + reservation_bytes;
            let strings_offset = struct_offset + self.structure.len();
            let totalsize = strings_offset + self.strings.len();
            let mut blob = Vec::with_capacity(totalsize);
            blob.extend_from_slice(&FDT_MAGIC.to_be_bytes());
            blob.extend_from_slice(&(totalsize as u32).to_be_bytes());
            blob.extend_from_slice(&(struct_offset as u32).to_be_bytes());
            blob.extend_from_slice(&(strings_offset as u32).to_be_bytes());
            blob.extend_from_slice(&40u32.to_be_bytes());
            blob.extend_from_slice(&17u32.to_be_bytes());
            blob.extend_from_slice(&16u32.to_be_bytes());
            blob.extend_from_slice(&0u32.to_be_bytes());
            blob.extend_from_slice(&(self.strings.len() as u32).to_be_bytes());
            blob.extend_from_slice(&(self.structure.len() as u32).to_be_bytes());
            for (address, size) in &self.reservations {
                blob.extend_from_slice(&address.to_be_bytes());
                blob.extend_from_slice(&size.to_be_bytes());
            }
            blob.extend_from_slice(&[0u8; 16]);
            blob.extend_from_slice(&self.structure);
            blob.extend_from_slice(&self.strings);
            blob
        }
    }

    fn pi_tree() -> Vec<u8> {
        let mut builder = Builder::new();
        builder.reservation(0x0, 0x1000);
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.string_prop("model", "Raspberry Pi 4 Model B Rev 1.4");
        builder.string_prop("compatible", "raspberrypi,4-model-b");

        builder.begin("memory@0");
        builder.string_prop("device_type", "memory");
        builder.cells_prop("reg", &[0, 0, 0x2000_0000]);
        builder.end();

        builder.begin("memory@100000000");
        builder.string_prop("device_type", "memory");
        builder.cells_prop("reg", &[1, 0, 0x1000_0000]);
        builder.end();

        builder.begin("reserved-memory");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.begin("cma");
        builder.string_prop("compatible", "shared-dma-pool");
        builder.cells_prop("reg", &[0, 0x100_0000, 0x20_0000]);
        builder.end();
        builder.end();

        builder.begin("chosen");
        builder.string_prop("bootargs", "console=ttyAMA0 test-exit=1");
        builder.cells_prop("linux,initrd-start", &[0, 0x500_0000]);
        builder.cells_prop("linux,initrd-end", &[0, 0x500_1000]);
        builder.end();

        builder.begin("soc");
        builder.cells_prop("#address-cells", &[1]);
        builder.cells_prop("#size-cells", &[1]);
        builder.cells_prop(
            "ranges",
            &[
                0x7e00_0000,
                0,
                0xfe00_0000,
                0x0180_0000,
                0x4000_0000,
                0,
                0xff80_0000,
                0x0080_0000,
            ],
        );

        builder.begin("interrupt-controller@40041000");
        builder.string_prop("compatible", "arm,gic-400");
        builder.cells_prop(
            "reg",
            &[
                0x4004_1000,
                0x1000,
                0x4004_2000,
                0x2000,
                0x4004_4000,
                0x2000,
                0x4004_6000,
                0x2000,
            ],
        );
        builder.end();

        builder.begin("serial@7e201000");
        builder.string_prop("compatible", "arm,pl011-axi");
        builder.cells_prop("reg", &[0x7e20_1000, 0x200]);
        builder.end();
        builder.end();

        builder.begin("timer");
        builder.string_prop("compatible", "arm,armv8-timer");
        builder.cells_prop(
            "interrupts",
            &[1, 13, 0xf08, 1, 14, 0xf08, 1, 11, 0xf08, 1, 10, 0xf08],
        );
        builder.end();

        builder.finish()
    }

    fn qemu_tree() -> Vec<u8> {
        let mut builder = Builder::new();
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[2]);
        builder.string_prop("model", "linux,dummy-virt");

        builder.begin("memory@40000000");
        builder.string_prop("device_type", "memory");
        builder.cells_prop("reg", &[0, 0x4000_0000, 0, 0x2000_0000]);
        builder.end();

        builder.begin("intc@8000000");
        builder.string_prop("compatible", "arm,cortex-a15-gic");
        builder.cells_prop(
            "reg",
            &[0, 0x0800_0000, 0, 0x1_0000, 0, 0x0801_0000, 0, 0x1_0000],
        );
        builder.end();

        builder.begin("pl011@9000000");
        builder.string_prop("compatible", "arm,pl011");
        builder.cells_prop("reg", &[0, 0x0900_0000, 0, 0x1000]);
        builder.end();

        builder.begin("timer");
        builder.string_prop("compatible", "arm,armv8-timer");
        builder.cells_prop(
            "interrupts",
            &[1, 13, 0xf08, 1, 14, 0xf08, 1, 11, 0xf08, 1, 10, 0xf08],
        );
        builder.end();

        builder.finish()
    }

    fn params() -> Params {
        Params {
            hhdm_offset: HHDM,
            kernel_phys_base: 0x8020_0000,
            kernel_virt_base: KERNEL,
            kernel_size: 0x20_0000,
            image_phys_base: 0x0008_0000,
            image_size: 0x30_0000,
            dtb_phys: 0x2eff_0000,
            dtb_len: 0x1_0000,
            initfs: PhysRange {
                base: 0x600_0000,
                len: 0x20_0000,
            },
            boot_cpu_id: 0x8100_0000,
            counter_freq: 54_000_000,
            entry_counter: 0x1234,
            memory_map_phys: 0x4000_0000,
            cmdline_phys: 0x4000_1000,
        }
    }

    fn map() -> [MemoryRegion; MAX_REGIONS] {
        [MemoryRegion {
            base: 0,
            len: 0,
            kind: 0,
            reserved: 0,
        }; MAX_REGIONS]
    }

    fn build(blob: &[u8], params: &Params) -> (BootInfo, [MemoryRegion; MAX_REGIONS], Vec<u8>) {
        let fdt = Fdt::parse(blob).expect("valid test tree");
        let mut regions = map();
        let mut cmdline = vec![0u8; CMDLINE_MAX];
        let info = build_into(&fdt, params, &mut regions, &mut cmdline).expect("build");
        (info, regions, cmdline)
    }

    fn kind_at(regions: &[MemoryRegion], info: &BootInfo, address: u64) -> Option<MemoryKind> {
        regions
            .iter()
            .take(usize::try_from(info.memory_map_len).unwrap_or(0))
            .find(|region| {
                address >= region.base && address < region.base.saturating_add(region.len)
            })
            .and_then(|region| region.kind())
    }

    #[test]
    fn bootinfo_header_is_filled() {
        let (info, _, _) = build(&pi_tree(), &params());
        assert_eq!(info.magic, BOOTINFO_MAGIC);
        assert_eq!(info.version, BOOTINFO_VERSION);
        assert_eq!(info.arch, arch::AARCH64);
        assert_eq!(info.hhdm_offset, HHDM);
        assert_eq!(info.kernel_virt_base, KERNEL);
        assert_eq!(info.memory_map_entry_size, 24);
        assert_eq!(info.dtb.base, 0x2eff_0000);
        assert_eq!(info.dtb.len, 0x1_0000);
        assert_eq!(info.counter_freq, 54_000_000);
    }

    #[test]
    fn memory_map_is_sorted_non_overlapping_and_valid() {
        let (info, regions, _) = build(&pi_tree(), &params());
        let length = usize::try_from(info.memory_map_len).unwrap();
        let map = &regions[..length];
        assert!(validate_memory_map(map).is_ok());
        for pair in map.windows(2) {
            assert!(pair[0].base <= pair[1].base);
        }
    }

    #[test]
    fn kernel_image_is_kernel_and_modules() {
        let (info, regions, _) = build(&pi_tree(), &params());
        assert_eq!(
            kind_at(&regions, &info, 0x80000),
            Some(MemoryKind::KernelAndModules)
        );
    }

    #[test]
    fn dtb_is_reserved() {
        let (info, regions, _) = build(&pi_tree(), &params());
        assert_eq!(
            kind_at(&regions, &info, 0x2eff_0000),
            Some(MemoryKind::Reserved)
        );
    }

    #[test]
    fn chosen_initrd_overrides_the_fallback() {
        let (info, regions, _) = build(&pi_tree(), &params());
        assert_eq!(info.initfs.base, 0x500_0000);
        assert_eq!(info.initfs.len, 0x1000);
        assert_eq!(
            kind_at(&regions, &info, 0x500_0000),
            Some(MemoryKind::KernelAndModules)
        );
    }

    #[test]
    fn fallback_initfs_is_used_without_chosen_initrd() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.begin("memory@0");
        builder.string_prop("device_type", "memory");
        builder.cells_prop("reg", &[0, 0, 0x1000_0000]);
        builder.end();
        let blob = builder.finish();
        let (info, _, _) = build(&blob, &params());
        assert_eq!(info.initfs.base, 0x600_0000);
        assert_eq!(info.initfs.len, 0x20_0000);
    }

    #[test]
    fn bootargs_are_copied_without_the_terminator() {
        let (info, _, cmdline) = build(&pi_tree(), &params());
        let length = usize::try_from(info.cmdline.len).unwrap();
        assert_eq!(&cmdline[..length], b"console=ttyAMA0 test-exit=1");
    }

    #[test]
    fn bootargs_that_do_not_fit_are_rejected() {
        let blob = pi_tree();
        let fdt = Fdt::parse(&blob).unwrap();
        let mut regions = map();
        let mut cmdline = [0u8; 4];
        assert_eq!(
            build_into(&fdt, &params(), &mut regions, &mut cmdline),
            Err(Error::CmdlineTooLong)
        );
    }

    #[test]
    fn empty_buffers_are_rejected() {
        let blob = pi_tree();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            build_into(&fdt, &params(), &mut [], &mut [0u8; 8]),
            Err(Error::EmptyBuffer)
        );
    }

    #[test]
    fn tree_without_memory_is_rejected() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            build_into(&fdt, &params(), &mut map(), &mut [0u8; 64]),
            Err(Error::NoMemory)
        );
    }

    #[test]
    fn too_small_map_buffer_is_rejected() {
        let blob = pi_tree();
        let fdt = Fdt::parse(&blob).unwrap();
        let mut regions = [MemoryRegion {
            base: 0,
            len: 0,
            kind: 0,
            reserved: 0,
        }; 2];
        assert_eq!(
            build_into(&fdt, &params(), &mut regions, &mut [0u8; CMDLINE_MAX]),
            Err(Error::TooManyRegions)
        );
    }

    #[test]
    fn reservation_block_splits_usable_memory() {
        let mut builder = Builder::new();
        builder.reservation(0x1000, 0x1000);
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.begin("memory@0");
        builder.string_prop("device_type", "memory");
        builder.cells_prop("reg", &[0, 0, 0x3000]);
        builder.end();
        let blob = builder.finish();
        let (info, regions, _) = build(&blob, &params());
        assert_eq!(kind_at(&regions, &info, 0x0), Some(MemoryKind::Usable));
        assert_eq!(kind_at(&regions, &info, 0x1000), None);
        assert_eq!(kind_at(&regions, &info, 0x2000), Some(MemoryKind::Usable));
    }

    #[test]
    fn reserved_memory_node_is_carved_out() {
        let (info, regions, _) = build(&pi_tree(), &params());
        assert_eq!(kind_at(&regions, &info, 0x100_0000), None);
    }

    #[test]
    fn mmio_uart_and_gic_are_mapped() {
        let (info, regions, _) = build(&pi_tree(), &params());
        assert_eq!(
            kind_at(&regions, &info, 0xfe20_1000),
            Some(MemoryKind::Mmio)
        );
        assert_eq!(
            kind_at(&regions, &info, 0xff84_1000),
            Some(MemoryKind::Mmio)
        );
        assert_eq!(
            kind_at(&regions, &info, 0xff84_2000),
            Some(MemoryKind::Mmio)
        );
    }

    #[test]
    fn platform_translates_pi_soc_ranges() {
        let blob = pi_tree();
        let fdt = Fdt::parse(&blob).unwrap();
        let platform = platform(&fdt);
        assert_eq!(platform.gicd, 0xff84_1000);
        assert_eq!(platform.gicc, 0xff84_2000);
        assert_eq!(platform.uart, 0xfe20_1000);
        assert_eq!(platform.uart_clock_hz, 48_000_000);
        assert_eq!(platform.timer_ppi, 30);
        assert!(platform.is_raspberry_pi());
        assert_eq!(
            platform.board_model(),
            Some("Raspberry Pi 4 Model B Rev 1.4")
        );
    }

    #[test]
    fn platform_reads_qemu_virt_root_nodes() {
        let blob = qemu_tree();
        let fdt = Fdt::parse(&blob).unwrap();
        let platform = platform(&fdt);
        assert_eq!(platform.gicd, 0x0800_0000);
        assert_eq!(platform.gicc, 0x0801_0000);
        assert_eq!(platform.uart, 0x0900_0000);
        assert_eq!(platform.uart_clock_hz, 24_000_000);
        assert_eq!(platform.timer_ppi, 30);
        assert!(!platform.is_raspberry_pi());
    }

    #[test]
    fn platform_defaults_without_nodes() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let platform = platform(&fdt);
        assert_eq!(platform.gicd, 0x0800_0000);
        assert_eq!(platform.uart, 0x0900_0000);
        assert_eq!(platform.timer_ppi, 30);
        assert!(!platform.from_dtb);
    }

    #[test]
    fn unaligned_memory_range_is_page_rounded() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.begin("memory@0");
        builder.string_prop("device_type", "memory");
        builder.cells_prop("reg", &[0, 0x1234, 0x2000]);
        builder.end();
        let blob = builder.finish();
        let (info, regions, _) = build(&blob, &params());
        let map = &regions[..usize::try_from(info.memory_map_len).unwrap()];
        assert!(map.iter().all(|region| region.base.is_multiple_of(4096)));
        assert!(map.iter().all(|region| region.len.is_multiple_of(4096)));
    }

    #[test]
    fn malformed_memory_reg_is_skipped() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.cells_prop("#address-cells", &[2]);
        builder.cells_prop("#size-cells", &[1]);
        builder.begin("memory@0");
        builder.string_prop("device_type", "memory");
        builder.prop("reg", &[0, 0, 0]);
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        assert_eq!(
            build_into(&fdt, &params(), &mut map(), &mut [0u8; 64]),
            Err(Error::NoMemory)
        );
    }

    #[test]
    fn model_is_truncated_without_panicking() {
        let mut builder = Builder::new();
        builder.begin("");
        builder.string_prop("model", &"x".repeat(200));
        builder.end();
        let blob = builder.finish();
        let fdt = Fdt::parse(&blob).unwrap();
        let platform = platform(&fdt);
        assert_eq!(platform.board_model().map(str::len), Some(64));
    }

    #[test]
    fn cmdline_length_is_bounded_by_the_buffer() {
        let (info, _, _) = build(&pi_tree(), &params());
        assert_eq!(
            info.cmdline.base, 0x4000_1000,
            "the caller's physical buffer address is recorded"
        );
        assert!(info.cmdline.len as usize <= CMDLINE_MAX);
    }
}
