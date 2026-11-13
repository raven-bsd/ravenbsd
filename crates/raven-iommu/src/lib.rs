// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Intel VT-d tables and data structures.
//!
//! The crate is deliberately split from the kernel: DMAR parsing, root/context
//! entry encoding, second-level page-table indexing, the IOVA allocator and the
//! fault-record decoder are all host-testable. The kernel maps the registers and
//! owns the physical pages; nothing here touches hardware.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

/// DMAR table header size (ACPI SDT header plus the DMAR fields).
pub const DMAR_HEADER_LEN: usize = 48;
/// One root-table entry is 16 bytes.
pub const ROOT_ENTRY_LEN: usize = 16;
/// One context-table entry is 16 bytes.
pub const CONTEXT_ENTRY_LEN: usize = 16;
/// VT-d translation page size.
pub const PAGE_SIZE: u64 = 4096;
/// Entries in one translation table page.
pub const TABLE_ENTRIES: usize = 512;

/// ACPI DMAR remapping-structure type: DMA Remapping Hardware Unit Definition.
pub const DMAR_DRHD: u16 = 0;
/// ACPI DMAR remapping-structure type: Reserved Memory Region Reporting.
pub const DMAR_RMRR: u16 = 1;
/// ACPI DMAR remapping-structure type: Root Port ATS Capability Reporting.
pub const DMAR_ATSR: u16 = 2;

/// Root-entry present bit.
pub const ROOT_PRESENT: u64 = 1;
/// Context-entry present bit.
pub const CONTEXT_PRESENT: u64 = 1;
/// Context-entry translation type: untranslated (second-level tables).
pub const CONTEXT_T_UNTRANSLATED: u64 = 0;
/// Context-entry address-space-root mask.
pub const CONTEXT_ASR_MASK: u64 = 0x000F_FFFF_FFFF_F000;
/// Page-table entry present bit.
pub const PTE_PRESENT: u64 = 1;
/// Page-table entry writable bit.
pub const PTE_WRITE: u64 = 1 << 1;
/// Page-table entry address mask (4 KiB pages).
pub const PTE_ADDRESS_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// The source-id fits in the low 16 bits of a fault record.
pub const FAULT_SID_MASK: u16 = 0xFFFF;

/// Why a DMAR table was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmarError {
    /// The table is shorter than its fixed header.
    TooShort,
    /// The ACPI signature is not `DMAR`.
    BadSignature,
    /// The ACPI checksum is not zero.
    BadChecksum,
    /// A remapping structure has a bad length or lies outside the table.
    BadLength,
    /// A DRHD has a zero or misaligned register base, or an unsupported segment.
    BadUnit,
    /// The table contains no DMA remapping unit.
    NoUnits,
}

impl fmt::Display for DmarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooShort => "DMAR table is too short",
            Self::BadSignature => "bad DMAR signature",
            Self::BadChecksum => "bad DMAR checksum",
            Self::BadLength => "bad DMAR structure length",
            Self::BadUnit => "bad or unsupported DRHD",
            Self::NoUnits => "DMAR has no remapping unit",
        })
    }
}

/// One device-scope entry inside a DRHD, RMRR or ATSR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceScope {
    /// Scope type (1 PCI endpoint, 2 PCI sub-hierarchy, 3 IOAPIC, ...).
    pub kind: u8,
    /// Enumeration id for IOAPIC/HPET scopes.
    pub enumeration: u8,
    /// First bus in the scope.
    pub start_bus: u8,
    /// Path bytes (device/function for PCI endpoints).
    pub path: [u8; 2],
}

/// One DMA Remapping Hardware Unit Definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drhd {
    /// PCI segment number.
    pub segment: u16,
    /// Register base address.
    pub base: u64,
    /// The unit covers all PCI devices not named by another DRHD.
    pub include_all: bool,
    /// Device scopes attached to this unit.
    pub scopes: Vec<DeviceScope>,
}

/// One Reserved Memory Region Reporting structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rmrr {
    /// PCI segment number.
    pub segment: u16,
    /// First reserved physical byte.
    pub base: u64,
    /// Last reserved physical byte (inclusive in the ACPI table).
    pub limit: u64,
    /// Device scopes attached to the region.
    pub scopes: Vec<DeviceScope>,
}

/// One Root Port ATS Capability Reporting structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atsr {
    /// PCI segment number.
    pub segment: u16,
    /// All ports in the segment support ATS.
    pub all_ports: bool,
    /// Device scopes attached to the structure.
    pub scopes: Vec<DeviceScope>,
}

/// A parsed ACPI DMAR table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmarTable {
    /// Host address width in bits (the ACPI field's low six bits).
    pub host_address_width: u8,
    /// Raw flag byte: bit 0 interrupt remapping, bit 1 x2APIC opt-out, bit 2 DMA control.
    pub flags: u8,
    /// DMA remapping units in table order.
    pub units: Vec<Drhd>,
    /// Reserved memory regions.
    pub rmrr: Vec<Rmrr>,
    /// Root-port ATS capabilities.
    pub atsr: Vec<Atsr>,
}

impl DmarTable {
    /// Parse and validate one complete DMAR table.
    pub fn parse(bytes: &[u8]) -> Result<Self, DmarError> {
        if bytes.len() < DMAR_HEADER_LEN {
            return Err(DmarError::TooShort);
        }
        if bytes.get(..4) != Some(b"DMAR".as_slice()) {
            return Err(DmarError::BadSignature);
        }
        let length = read_u32(bytes, 4).ok_or(DmarError::TooShort)? as usize;
        if length < DMAR_HEADER_LEN || length > bytes.len() {
            return Err(DmarError::BadLength);
        }
        if !checksum(&bytes[..length]) {
            return Err(DmarError::BadChecksum);
        }

        let mut table = Self {
            host_address_width: bytes[36] & 0x3F,
            flags: bytes[37],
            units: Vec::new(),
            rmrr: Vec::new(),
            atsr: Vec::new(),
        };
        let mut offset = DMAR_HEADER_LEN;
        while offset < length {
            if offset.checked_add(4).is_none_or(|end| end > length) {
                return Err(DmarError::BadLength);
            }
            let kind = read_u16(bytes, offset).ok_or(DmarError::BadLength)?;
            let structure_len = read_u16(bytes, offset + 2).ok_or(DmarError::BadLength)? as usize;
            if structure_len < 8
                || offset
                    .checked_add(structure_len)
                    .is_none_or(|end| end > length)
            {
                return Err(DmarError::BadLength);
            }
            let body = &bytes[offset..offset + structure_len];
            match kind {
                DMAR_DRHD => table.units.push(parse_drhd(body)?),
                DMAR_RMRR => table.rmrr.push(parse_rmrr(body)?),
                DMAR_ATSR => table.atsr.push(parse_atsr(body)?),
                _ => {}
            }
            offset += structure_len;
        }
        if table.units.is_empty() {
            return Err(DmarError::NoUnits);
        }
        Ok(table)
    }

    /// The first remapping unit (the kernel currently drives one unit).
    pub fn first_unit(&self) -> &Drhd {
        &self.units[0]
    }

    /// Number of RMRR ranges that must be accounted for.
    pub fn rmrr_count(&self) -> usize {
        self.rmrr.len()
    }
}

fn parse_drhd(body: &[u8]) -> Result<Drhd, DmarError> {
    if body.len() < 16 {
        return Err(DmarError::BadLength);
    }
    let flags = body[4];
    let segment = read_u16(body, 6).ok_or(DmarError::BadLength)?;
    let base = read_u64(body, 8).ok_or(DmarError::BadLength)?;
    if base == 0 || base & 0xFFF != 0 {
        return Err(DmarError::BadUnit);
    }
    if segment != 0 {
        return Err(DmarError::BadUnit);
    }
    Ok(Drhd {
        segment,
        base,
        include_all: flags & 1 != 0,
        scopes: parse_scopes(body, 16)?,
    })
}

fn parse_rmrr(body: &[u8]) -> Result<Rmrr, DmarError> {
    if body.len() < 24 {
        return Err(DmarError::BadLength);
    }
    let segment = read_u16(body, 4).ok_or(DmarError::BadLength)?;
    let base = read_u64(body, 8).ok_or(DmarError::BadLength)?;
    let limit = read_u64(body, 16).ok_or(DmarError::BadLength)?;
    if limit < base {
        return Err(DmarError::BadLength);
    }
    Ok(Rmrr {
        segment,
        base,
        limit,
        scopes: parse_scopes(body, 24)?,
    })
}

fn parse_atsr(body: &[u8]) -> Result<Atsr, DmarError> {
    if body.len() < 8 {
        return Err(DmarError::BadLength);
    }
    let flags = body[4];
    let segment = read_u16(body, 6).ok_or(DmarError::BadLength)?;
    Ok(Atsr {
        segment,
        all_ports: flags & 1 != 0,
        scopes: parse_scopes(body, 8)?,
    })
}

fn parse_scopes(body: &[u8], mut offset: usize) -> Result<Vec<DeviceScope>, DmarError> {
    let mut scopes = Vec::new();
    while offset < body.len() {
        if offset.checked_add(8).is_none_or(|end| end > body.len()) {
            return Err(DmarError::BadLength);
        }
        let kind = body[offset];
        let scope_len = body[offset + 1] as usize;
        if scope_len < 8
            || offset
                .checked_add(scope_len)
                .is_none_or(|end| end > body.len())
        {
            return Err(DmarError::BadLength);
        }
        scopes.push(DeviceScope {
            kind,
            enumeration: body[offset + 4],
            start_bus: body[offset + 5],
            path: [body[offset + 6], body[offset + 7]],
        });
        offset += scope_len;
    }
    Ok(scopes)
}

fn checksum(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([*slice.first()?, *slice.get(1)?]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *slice.first()?,
        *slice.get(1)?,
        *slice.get(2)?,
        *slice.get(3)?,
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let low = u64::from(read_u32(bytes, offset)?);
    let high = u64::from(read_u32(bytes, offset.checked_add(4)?)?);
    Some(low | (high << 32))
}

/// Root-entry encoding: present plus a page-aligned context table.
pub fn root_entry(context_phys: u64) -> u64 {
    ROOT_PRESENT | (context_phys & CONTEXT_ASR_MASK)
}

/// Context-entry encoding: present, untranslated second-level tables, domain id and AGAW.
pub fn context_entry(domain: u16, levels: u8, table_phys: u64) -> (u64, u64) {
    let low = CONTEXT_PRESENT | CONTEXT_T_UNTRANSLATED | (table_phys & CONTEXT_ASR_MASK);
    let high = (u64::from(domain) << 8) | u64::from(agaw_code(levels));
    (low, high)
}

/// VT-d AGAW code for a page-table level count.
pub fn agaw_code(levels: u8) -> u8 {
    match levels {
        4 => 2,
        3 => 1,
        _ => 0,
    }
}

/// Page-table level count for an AGAW value in bits (30, 39, 48).
pub fn levels_for_agaw(agaw: u8) -> Option<u8> {
    match agaw {
        30 => Some(2),
        39 => Some(3),
        48 => Some(4),
        _ => None,
    }
}

/// Index into the table at `level` for `levels` total levels.
pub fn table_index(levels: u8, level: u8, address: u64) -> usize {
    let shift = 12 + (u32::from(levels) - u32::from(level) - 1) * 9;
    ((address >> shift) as usize) & (TABLE_ENTRIES - 1)
}

/// Page-table entry for a 4 KiB leaf.
pub fn page_entry(phys: u64, writable: bool) -> u64 {
    PTE_PRESENT | (phys & PTE_ADDRESS_MASK) | if writable { PTE_WRITE } else { 0 }
}

/// A page-granular first-fit IOVA allocator.
#[derive(Clone, Debug)]
pub struct IovaAllocator {
    start: u64,
    end: u64,
    free: Vec<(u64, u64)>,
}

/// Why an IOVA release was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IovaError {
    /// The range is not page aligned or has zero length.
    Misaligned,
    /// The range was not allocated from this allocator.
    NotAllocated,
}

impl IovaAllocator {
    /// Create an allocator over `[start, end)` (both page aligned).
    pub fn new(start: u64, end: u64) -> Result<Self, IovaError> {
        if start >= end || !start.is_multiple_of(PAGE_SIZE) || !end.is_multiple_of(PAGE_SIZE) {
            return Err(IovaError::Misaligned);
        }
        Ok(Self {
            start,
            end,
            free: alloc::vec![(start, end)],
        })
    }

    /// First address in the managed range.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// One past the managed range.
    pub fn end(&self) -> u64 {
        self.end
    }

    /// Free bytes remaining.
    pub fn free_bytes(&self) -> u64 {
        self.free.iter().fold(0u64, |total, (start, end)| {
            total.saturating_add(end - start)
        })
    }

    /// Allocate `len` bytes with `align` byte alignment; returns the IOVA.
    pub fn allocate(&mut self, len: u64, align: u64) -> Option<u64> {
        self.allocate_within(len, align, self.end)
    }

    /// Allocate `len` bytes below `limit` (and inside the managed range).
    pub fn allocate_within(&mut self, len: u64, align: u64, limit: u64) -> Option<u64> {
        if len == 0
            || !len.is_multiple_of(PAGE_SIZE)
            || !align.is_multiple_of(PAGE_SIZE)
            || !align.is_power_of_two()
        {
            return None;
        }
        let limit = limit.min(self.end);
        let mut index = 0usize;
        while index < self.free.len() {
            let (range_start, range_end) = *self.free.get(index)?;
            let candidate = align_up(range_start, align);
            let candidate_end = candidate.checked_add(len)?;
            if candidate_end <= range_end.min(limit) {
                if candidate == range_start {
                    if candidate_end == range_end {
                        self.free.remove(index);
                    } else if let Some(slot) = self.free.get_mut(index) {
                        *slot = (candidate_end, range_end);
                    }
                } else {
                    if let Some(slot) = self.free.get_mut(index) {
                        *slot = (candidate_end, range_end);
                    }
                    self.free.insert(index, (range_start, candidate));
                }
                return Some(candidate);
            }
            index += 1;
        }
        None
    }

    /// Release a previously allocated IOVA range and coalesce neighbours.
    pub fn release(&mut self, base: u64, len: u64) -> Result<(), IovaError> {
        if len == 0 || !base.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(IovaError::Misaligned);
        }
        let end = base.checked_add(len).ok_or(IovaError::Misaligned)?;
        if base < self.start || end > self.end {
            return Err(IovaError::NotAllocated);
        }
        let mut index = 0usize;
        while index < self.free.len() {
            let (start, _) = *self.free.get(index).ok_or(IovaError::NotAllocated)?;
            if start >= base {
                break;
            }
            index += 1;
        }
        self.free.insert(index, (base, end));
        self.free.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(self.free.len());
        for (start, end) in self.free.drain(..) {
            if let Some(last) = merged.last_mut()
                && last.1 == start
            {
                last.1 = end;
            } else {
                merged.push((start, end));
            }
        }
        self.free = merged;
        Ok(())
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    value
        .checked_add(align - 1)
        .map(|value| value & !(align - 1))
        .unwrap_or(u64::MAX)
}

/// One decoded VT-d fault-record entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaultRecord {
    /// The fault-present bit in the high word.
    pub present: bool,
    /// True for a write or atomic request.
    pub write: bool,
    /// Fault address from the low word.
    pub address: u64,
    /// Fault reason code (high word bits 32..39).
    pub reason: u8,
    /// Source id (bus/device/function).
    pub source_id: u16,
    /// Address type from the high word bits 60..61.
    pub address_type: u8,
}

/// Decode one fault-recording register pair.
pub fn decode_fault(low: u64, high: u64) -> FaultRecord {
    FaultRecord {
        present: high >> 63 != 0,
        write: (high >> 62) & 1 != 0,
        address: low & PTE_ADDRESS_MASK,
        reason: ((high >> 32) & 0xFF) as u8,
        source_id: (high & u64::from(FAULT_SID_MASK)) as u16,
        address_type: ((high >> 60) & 0x3) as u8,
    }
}

/// Human-readable fault reason for the common VT-d codes.
pub fn fault_reason_text(reason: u8) -> &'static str {
    match reason {
        0x01 => "root-entry not present",
        0x02 => "context-entry not present",
        0x03 => "context-entry invalid",
        0x04 => "second-level entry not present",
        0x05 => "reserved bit set",
        0x06 => "invalid request",
        0x07 => "invalid address",
        _ => "other VT-d fault",
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
