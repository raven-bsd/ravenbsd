// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure helpers for the RavenBSD first-stage direct-boot loader (S89).
//!
//! The loader is a small PVH ELF under `boot/direct` that QEMU can start with `-kernel`. It
//! consumes one `-initrd` module built by `scripts/boot-direct.sh`: a header, the Raven kernel
//! ELF and the release initfs. The code here parses the PVH start information, the payload and
//! the module, and converts the PVH memory map into the kernel's `MemoryRegion` contract.
//!
//! Everything in this crate is pure (no I/O, no physical-memory access) so it can be unit
//! tested on the host. The binary under `src/main.rs` provides the physical access and the
//! 32-bit PVH entry.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

use raven_bootinfo::{MemoryKind, MemoryRegion, PhysRange};

/// Size of the payload header written by `scripts/boot-direct.sh`.
pub const PAYLOAD_HEADER_SIZE: usize = 16;
/// Payload magic: eight ASCII bytes followed by a little-endian `u64` kernel length.
pub const PAYLOAD_MAGIC: [u8; 8] = *b"RVDIRECT";
/// PVH `hvm_start_info` magic (`XEN_HVM_START_MAGIC_VALUE`).
pub const HVM_START_MAGIC: u32 = 0x336e_c578;
/// Size of one `hvm_memmap_table_entry`.
pub const HVM_MEMMAP_ENTRY_SIZE: usize = 24;
/// Size of one `hvm_modlist_entry`.
pub const HVM_MODULE_SIZE: usize = 32;
/// Physical addresses below this boundary stay `Reserved` in the kernel map.
pub const LOW_MEMORY_END: u64 = 0x10_0000;
/// Maximum memory-map entries the kernel's static array accepts (`MAX_MEMORY_MAP`).
pub const MAX_MEMORY_MAP: usize = 512;
/// Maximum number of page-aligned boundaries considered by [`build_memory_map`].
pub const MAX_BOUNDARIES: usize = 512;

/// Why a payload could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadError {
    /// The payload is shorter than the header.
    TooShort,
    /// The eight magic bytes do not match [`PAYLOAD_MAGIC`].
    BadMagic,
    /// The kernel length does not fit the module.
    BadKernelLength,
}

/// The two slices of a direct-boot payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload<'a> {
    /// The Raven kernel ELF.
    pub kernel: &'a [u8],
    /// The release initfs image.
    pub initfs: &'a [u8],
}

/// Parse the payload produced by `scripts/boot-direct.sh`.
///
/// Layout: `RVDIRECT` (8 bytes), kernel length (little-endian `u64`), kernel ELF, initfs. The
/// initfs runs to the end of the module.
pub fn parse_payload(data: &[u8]) -> Result<Payload<'_>, PayloadError> {
    if data.len() < PAYLOAD_HEADER_SIZE {
        return Err(PayloadError::TooShort);
    }
    if data.get(0..8) != Some(PAYLOAD_MAGIC.as_slice()) {
        return Err(PayloadError::BadMagic);
    }
    let kernel_len = read_u64(data, 8).ok_or(PayloadError::TooShort)?;
    let kernel_len = usize::try_from(kernel_len).map_err(|_| PayloadError::BadKernelLength)?;
    let kernel_start = PAYLOAD_HEADER_SIZE;
    let kernel_end = kernel_start
        .checked_add(kernel_len)
        .ok_or(PayloadError::BadKernelLength)?;
    if kernel_end > data.len() {
        return Err(PayloadError::BadKernelLength);
    }
    Ok(Payload {
        kernel: &data[kernel_start..kernel_end],
        initfs: &data[kernel_end..],
    })
}

/// The PVH `hvm_start_info` fields the loader consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartInfo {
    /// [`HVM_START_MAGIC`].
    pub magic: u32,
    /// PVH ABI version (1 for QEMU 6.2).
    pub version: u32,
    /// Reserved flag bits.
    pub flags: u32,
    /// Number of module entries.
    pub nr_modules: u32,
    /// Physical address of the module list.
    pub modlist_paddr: u64,
    /// Physical address of the NUL-terminated command line.
    pub cmdline_paddr: u64,
    /// Physical address of the ACPI RSDP, or zero.
    pub rsdp_paddr: u64,
    /// Physical address of the memory-map array.
    pub memmap_paddr: u64,
    /// Number of memory-map entries.
    pub memmap_entries: u32,
}

/// Parse a 56-byte `hvm_start_info` structure.
pub fn parse_start_info(bytes: &[u8]) -> Option<StartInfo> {
    if bytes.len() < 56 {
        return None;
    }
    Some(StartInfo {
        magic: read_u32(bytes, 0)?,
        version: read_u32(bytes, 4)?,
        flags: read_u32(bytes, 8)?,
        nr_modules: read_u32(bytes, 12)?,
        modlist_paddr: read_u64(bytes, 16)?,
        cmdline_paddr: read_u64(bytes, 24)?,
        rsdp_paddr: read_u64(bytes, 32)?,
        memmap_paddr: read_u64(bytes, 40)?,
        memmap_entries: read_u32(bytes, 48)?,
    })
}

/// One PVH memory-map entry converted to the kernel's kind vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemmapEntry {
    /// Physical start address.
    pub addr: u64,
    /// Length in bytes.
    pub size: u64,
    /// Converted memory kind.
    pub kind: MemoryKind,
}

/// Why a PVH memory map could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The input slice is shorter than the requested number of entries.
    TooShort,
    /// The output array cannot hold all entries.
    TooManyEntries,
    /// An entry count or byte length overflowed.
    Overflow,
}

/// Decode `count` PVH memory-map entries into `out`.
pub fn parse_memmap_into(
    bytes: &[u8],
    count: u32,
    out: &mut [MemmapEntry],
) -> Result<usize, ParseError> {
    let count = usize::try_from(count).map_err(|_| ParseError::Overflow)?;
    if count > out.len() {
        return Err(ParseError::TooManyEntries);
    }
    let needed = count
        .checked_mul(HVM_MEMMAP_ENTRY_SIZE)
        .ok_or(ParseError::Overflow)?;
    if bytes.len() < needed {
        return Err(ParseError::TooShort);
    }
    for (index, slot) in out.iter_mut().take(count).enumerate() {
        let offset = index
            .checked_mul(HVM_MEMMAP_ENTRY_SIZE)
            .ok_or(ParseError::Overflow)?;
        let entry = bytes
            .get(offset..offset + HVM_MEMMAP_ENTRY_SIZE)
            .ok_or(ParseError::TooShort)?;
        let raw_kind = read_u32(entry, 16).ok_or(ParseError::TooShort)?;
        let kind = match raw_kind {
            1 => MemoryKind::Usable,
            2 => MemoryKind::Reserved,
            3 => MemoryKind::AcpiReclaimable,
            4 => MemoryKind::AcpiNvs,
            5 => MemoryKind::BadMemory,
            _ => MemoryKind::Reserved,
        };
        *slot = MemmapEntry {
            addr: read_u64(entry, 0).ok_or(ParseError::TooShort)?,
            size: read_u64(entry, 8).ok_or(ParseError::TooShort)?,
            kind,
        };
    }
    Ok(count)
}

/// The first PVH module (QEMU places `-initrd` there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Module {
    /// Physical address of the module bytes.
    pub paddr: u64,
    /// Module length in bytes.
    pub size: u64,
}

/// Parse the first entry of a PVH module list.
pub fn parse_module(bytes: &[u8]) -> Option<Module> {
    if bytes.len() < HVM_MODULE_SIZE {
        return None;
    }
    Some(Module {
        paddr: read_u64(bytes, 0)?,
        size: read_u64(bytes, 8)?,
    })
}

/// A physical range the loader wants reflected in the kernel memory map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackedRange {
    /// Physical start address.
    pub base: u64,
    /// Length in bytes.
    pub len: u64,
    /// Kind to assign to the range.
    pub kind: MemoryKind,
}

/// Why a memory map could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    /// The boundary array is too small for the input.
    TooManyBoundaries,
    /// The output array is too small for the resulting regions.
    TooManyRegions,
    /// An address or length overflowed.
    Overflow,
}

/// Convert the PVH memory map plus loader-owned ranges into the kernel's sorted, page-aligned
/// [`MemoryRegion`] array.
///
/// The conversion splits every input at 4 KiB boundaries and at every tracked range boundary,
/// forces everything below [`LOW_MEMORY_END`] to `Reserved`, and overlays tracked ranges last.
pub fn build_memory_map(
    entries: &[MemmapEntry],
    tracked: &[TrackedRange],
    out: &mut [MemoryRegion],
) -> Result<usize, MapError> {
    let mut bounds = [0u64; MAX_BOUNDARIES];
    let mut used = 0usize;
    push_bound(&mut bounds, &mut used, 0)?;
    push_bound(&mut bounds, &mut used, LOW_MEMORY_END)?;
    for entry in entries {
        let end = entry
            .addr
            .checked_add(entry.size)
            .ok_or(MapError::Overflow)?;
        push_bound(&mut bounds, &mut used, page_floor(entry.addr))?;
        push_bound(
            &mut bounds,
            &mut used,
            page_ceil(end).ok_or(MapError::Overflow)?,
        )?;
    }
    for range in tracked {
        let end = range
            .base
            .checked_add(range.len)
            .ok_or(MapError::Overflow)?;
        push_bound(&mut bounds, &mut used, page_floor(range.base))?;
        push_bound(
            &mut bounds,
            &mut used,
            page_ceil(end).ok_or(MapError::Overflow)?,
        )?;
    }

    insertion_sort(&mut bounds[..used]);
    let unique = dedup(&mut bounds[..used]);
    let mut count = 0usize;
    for window in 0..unique.saturating_sub(1) {
        let start = bounds[window];
        let end = bounds[window + 1];
        if end <= start {
            continue;
        }
        let mut kind = if start < LOW_MEMORY_END {
            MemoryKind::Reserved
        } else {
            kind_at(entries, start)
        };
        for range in tracked {
            let range_end = range.base.saturating_add(range.len);
            if range.base < end && start < range_end {
                kind = range.kind;
            }
        }
        let slot = out.get_mut(count).ok_or(MapError::TooManyRegions)?;
        *slot = MemoryRegion {
            base: start,
            len: end - start,
            kind: kind as u32,
            reserved: 0,
        };
        count += 1;
    }
    Ok(count)
}

/// Find a free physical range of `need` bytes in the PVH map, avoiding `avoid`.
///
/// Only `Usable` entries are considered and the result is aligned to 2 MiB so the kernel's
/// huge-page mapping has the best chance of using a leaf. Returns `None` when no range fits.
pub fn find_free_range(entries: &[MemmapEntry], need: u64, avoid: &[PhysRange]) -> Option<u64> {
    if need == 0 {
        return None;
    }
    const ALIGN: u64 = 0x20_0000;
    for entry in entries {
        if entry.kind != MemoryKind::Usable {
            continue;
        }
        let end = entry.addr.checked_add(entry.size)?;
        let mut start = align_up(page_ceil(entry.addr.max(LOW_MEMORY_END))?, ALIGN)?;
        for _ in 0..1024 {
            let candidate_end = start.checked_add(need)?;
            if candidate_end > end {
                break;
            }
            let mut advanced = false;
            for range in avoid {
                let range_end = range.base.checked_add(range.len)?;
                if range.base < candidate_end && start < range_end {
                    start = align_up(range_end.max(start.saturating_add(0x1000)), ALIGN)?;
                    advanced = true;
                    break;
                }
            }
            if !advanced {
                return Some(start);
            }
        }
    }
    None
}

fn kind_at(entries: &[MemmapEntry], address: u64) -> MemoryKind {
    for entry in entries {
        if entry.size == 0 {
            continue;
        }
        let end = entry.addr.saturating_add(entry.size);
        if address >= entry.addr && address < end {
            return entry.kind;
        }
    }
    MemoryKind::Reserved
}

fn push_bound(bounds: &mut [u64], used: &mut usize, value: u64) -> Result<(), MapError> {
    let slot = bounds.get_mut(*used).ok_or(MapError::TooManyBoundaries)?;
    *slot = value;
    *used += 1;
    Ok(())
}

fn insertion_sort(values: &mut [u64]) {
    for index in 1..values.len() {
        let value = values[index];
        let mut position = index;
        while position > 0 && values[position - 1] > value {
            values[position] = values[position - 1];
            position -= 1;
        }
        values[position] = value;
    }
}

fn dedup(values: &mut [u64]) -> usize {
    let mut unique = 0usize;
    for index in 0..values.len() {
        if unique == 0 || values[index] != values[unique - 1] {
            values[unique] = values[index];
            unique += 1;
        }
    }
    unique
}

fn page_floor(value: u64) -> u64 {
    value & !0xfff
}

fn page_ceil(value: u64) -> Option<u64> {
    value.checked_add(0xfff).map(page_floor)
}

fn align_up(value: u64, align: u64) -> Option<u64> {
    let mask = align.checked_sub(1)?;
    value.checked_add(mask).map(|rounded| rounded & !mask)
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice = bytes.get(offset..offset.checked_add(8)?)?;
    let array: [u8; 8] = slice.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    let array: [u8; 4] = slice.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

/// Size of an ACPI RSDP structure (revision 2).
pub const ACPI_RSDP_SIZE: usize = 36;
/// Size of an ACPI table header.
pub const ACPI_HEADER_SIZE: usize = 36;
/// Size of one MCFG allocation entry.
pub const MCFG_ALLOCATION_SIZE: usize = 16;
/// Size of the MCFG table this loader builds (one allocation).
pub const MCFG_TABLE_SIZE: usize = ACPI_HEADER_SIZE + 8 + MCFG_ALLOCATION_SIZE;

/// Why the MCFG injection could not build a new ACPI set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpiError {
    /// The RSDP or XSDT input is shorter than its header.
    TooShort,
    /// The RSDP signature is not `RSD PTR `.
    BadRsdpSignature,
    /// The XSDT signature is not `XSDT`.
    BadXsdtSignature,
    /// A checksum in the original tables is wrong.
    BadChecksum,
    /// The entry count does not fit the output buffer.
    OutputTooSmall,
    /// An offset or length overflowed.
    Overflow,
}

/// Append an MCFG table to a copy of `xsdt` and return the offset of the new RSDP in `out`.
///
/// `out` receives a complete ACPI set: a new revision-2 RSDP at offset 0, a new XSDT at offset
/// 64 with every original entry plus the MCFG pointer, and the MCFG table. `base_paddr` is the
/// physical address of `out[0]`; all internal pointers are written as physical addresses.
///
/// The caller must copy the existing tables into `xsdt` verbatim. The original RSDP/XSDT are not
/// modified. Existing MCFG detection is the caller's job (it requires reading the referenced
/// tables).
pub fn append_mcfg(
    rsdp: &[u8],
    xsdt: &[u8],
    base_paddr: u64,
    ecam_base: u64,
    out: &mut [u8],
) -> Result<usize, AcpiError> {
    if rsdp.len() < ACPI_RSDP_SIZE || xsdt.len() < ACPI_HEADER_SIZE {
        return Err(AcpiError::TooShort);
    }
    if rsdp.get(0..8) != Some(b"RSD PTR ") {
        return Err(AcpiError::BadRsdpSignature);
    }
    if xsdt.get(0..4) != Some(b"XSDT") {
        return Err(AcpiError::BadXsdtSignature);
    }
    if checksum(&rsdp[..20]) != 0 || checksum(&rsdp[..ACPI_RSDP_SIZE]) != 0 {
        return Err(AcpiError::BadChecksum);
    }
    let xsdt_len = usize::try_from(read_u32(xsdt, 4).ok_or(AcpiError::TooShort)?)
        .map_err(|_| AcpiError::Overflow)?;
    if xsdt_len < ACPI_HEADER_SIZE
        || xsdt_len > xsdt.len()
        || !(xsdt_len - ACPI_HEADER_SIZE).is_multiple_of(8)
    {
        return Err(AcpiError::TooShort);
    }
    if checksum(&xsdt[..xsdt_len]) != 0 {
        return Err(AcpiError::BadChecksum);
    }
    let entries = (xsdt_len - ACPI_HEADER_SIZE) / 8;

    let xsdt_offset = 64usize;
    let new_xsdt_len = ACPI_HEADER_SIZE
        .checked_add(
            entries
                .checked_add(1)
                .and_then(|value| value.checked_mul(8))
                .ok_or(AcpiError::Overflow)?,
        )
        .ok_or(AcpiError::Overflow)?;
    let mcfg_offset = xsdt_offset
        .checked_add(new_xsdt_len)
        .and_then(|value| value.checked_add(7).map(|value| value & !7))
        .ok_or(AcpiError::Overflow)?;
    let total = mcfg_offset
        .checked_add(MCFG_TABLE_SIZE)
        .ok_or(AcpiError::Overflow)?;
    if out.len() < total {
        return Err(AcpiError::OutputTooSmall);
    }
    out[..total].fill(0);

    let xsdt_paddr = base_paddr
        .checked_add(xsdt_offset as u64)
        .ok_or(AcpiError::Overflow)?;
    let mcfg_paddr = base_paddr
        .checked_add(mcfg_offset as u64)
        .ok_or(AcpiError::Overflow)?;

    // RSDP: copy the original identity fields and point at the new XSDT.
    out[..ACPI_RSDP_SIZE].copy_from_slice(&rsdp[..ACPI_RSDP_SIZE]);
    out[8] = 0;
    out[24..32].copy_from_slice(&xsdt_paddr.to_le_bytes());
    out[32] = 0;
    let legacy_checksum = checksum(&out[..20]);
    out[8] = legacy_checksum;
    let extended_checksum = checksum(&out[..ACPI_RSDP_SIZE]);
    out[32] = extended_checksum;

    // XSDT: copy the original header, replace length/checksum, copy entries, append MCFG.
    out[xsdt_offset..xsdt_offset + ACPI_HEADER_SIZE].copy_from_slice(&xsdt[..ACPI_HEADER_SIZE]);
    out[xsdt_offset + 4..xsdt_offset + 8].copy_from_slice(&(new_xsdt_len as u32).to_le_bytes());
    out[xsdt_offset + 9] = 0;
    out[xsdt_offset + ACPI_HEADER_SIZE..xsdt_offset + xsdt_len]
        .copy_from_slice(&xsdt[ACPI_HEADER_SIZE..xsdt_len]);
    let last = xsdt_offset + xsdt_len;
    out[last..last + 8].copy_from_slice(&mcfg_paddr.to_le_bytes());
    let xsdt_checksum = checksum(&out[xsdt_offset..xsdt_offset + new_xsdt_len]);
    out[xsdt_offset + 9] = xsdt_checksum;

    // MCFG: one allocation entry for the QEMU microvm GPEX ECAM window.
    let mcfg = &mut out[mcfg_offset..mcfg_offset + MCFG_TABLE_SIZE];
    mcfg[..4].copy_from_slice(b"MCFG");
    mcfg[4..8].copy_from_slice(&(MCFG_TABLE_SIZE as u32).to_le_bytes());
    mcfg[8] = 1;
    mcfg[10..16].copy_from_slice(b"RAVEN ");
    mcfg[16..24].copy_from_slice(b"RAVENMCF");
    mcfg[24..28].copy_from_slice(&1u32.to_le_bytes());
    mcfg[28..32].copy_from_slice(b"RAVE");
    mcfg[32..36].copy_from_slice(&1u32.to_le_bytes());
    mcfg[44..52].copy_from_slice(&ecam_base.to_le_bytes());
    mcfg[52..54].copy_from_slice(&0u16.to_le_bytes());
    mcfg[54] = 0;
    mcfg[55] = 255;
    let mcfg_checksum = checksum(&mcfg[..MCFG_TABLE_SIZE]);
    mcfg[9] = mcfg_checksum;

    Ok(0)
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes
        .iter()
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
        .wrapping_neg()
}
