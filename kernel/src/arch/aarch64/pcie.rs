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

use core::cell::UnsafeCell;

use raven_pci::{DiscoverError, Indirect, Rp1Device, Rp1Window, Scan, discover};
use raven_rmm::MapError;

use crate::kprintln;
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

// ---------------------------------------------------------------------------------------------
// aarch64 ACPI bundle for the QEMU `virt` PCIe host.
//
// The UEFI loader forwards the RSDP from the firmware configuration table (`BootInfo.acpi_rsdp`).
// The x86-64 early path copies the ACPI tables into a kernel-owned bundle before loader memory is
// reclaimed; aarch64 never captured them, so `pcid` saw no ACPI blob and the `pci:` scheme never
// appeared. The bundle format is the same `"RAC1"` directory `pcid::mcfg` parses; the QEMU `virt`
// MCFG names the `pci-host-ecam-generic` window.
//
// The capture runs from `driver::init_aarch64`, after the HHDM exists and before any server can
// read the blob. ACPI reclaim memory is not freed by the kernel, so the late capture is stable.

/// Upper bound of the captured ACPI bundle, including header, directory and table copies.
const MAX_ACPI_BLOB: usize = 256 * 1024;
/// Largest ACPI table copied into the bundle.
const MAX_TABLE_BYTES: u32 = 64 * 1024;
/// Largest number of bundle entries (RSDP, root tables and SDTs).
const MAX_BLOB_TABLES: usize = 64;
/// Magic of the kernel's ACPI bundle (`"RAC1"`); shared with `pcid`.
const ACPI_BLOB_MAGIC: u32 = 0x3143_4152;
/// ACPI bundle layout version.
const ACPI_BLOB_VERSION: u16 = 1;
/// Size of the ACPI bundle header in bytes.
const ACPI_BLOB_HEADER_SIZE: usize = 24;
/// Size of one ACPI bundle directory entry in bytes.
const ACPI_BLOB_ENTRY_SIZE: usize = 20;

/// The captured bundle and its used length.
struct AcpiBlob {
    bytes: [u8; MAX_ACPI_BLOB],
    len: usize,
}

struct AcpiCell(UnsafeCell<AcpiBlob>);

// SAFETY: the cell is written once on the boot CPU before the blob is published, and only read
// afterwards; the boot path is single-threaded until the scheduler starts.
unsafe impl Sync for AcpiCell {}

static ACPI: AcpiCell = AcpiCell(UnsafeCell::new(AcpiBlob {
    bytes: [0; MAX_ACPI_BLOB],
    len: 0,
}));

/// One bundle entry collected before the header is written.
#[derive(Clone, Copy)]
struct BlobEntry {
    signature: [u8; 8],
    physical: u64,
    length: u32,
}

/// Capture the firmware ACPI tables into a kernel-owned bundle for `pcid`.
///
/// `rsdp_phys` is the firmware RSDP from `BootInfo`. A zero or malformed RSDP leaves the blob
/// empty; `pcid` then reports its usual `acpi blob` error instead of this module guessing a
/// machine layout.
pub fn capture_acpi(rsdp_phys: u64) {
    // SAFETY: the single-CPU boot path is the only writer; the cell is unpublished until complete.
    let blob = unsafe { &mut *ACPI.0.get() };
    match build_acpi_blob(rsdp_phys, blob) {
        Ok(()) => kprintln!("pcie: acpi blob {} bytes rsdp=0x{:x}", blob.len, rsdp_phys),
        Err(reason) => {
            blob.len = 0;
            kprintln!("pcie: acpi blob unavailable ({reason})");
        }
    }
}

/// Capture from the boot handoff and return the published bundle for `BOOT_BLOB` kind 1.
#[must_use]
pub fn acpi_boot_blob() -> Option<&'static [u8]> {
    // SAFETY: paging published the copied BootInfo before `driver::init_aarch64` runs.
    let info = unsafe { &*crate::memory::paging::boot_info_ptr() };
    capture_acpi(info.acpi_rsdp);
    acpi_blob()
}

/// The captured ACPI bundle, or `None` when ACPI is absent or the capture failed.
#[must_use]
pub fn acpi_blob() -> Option<&'static [u8]> {
    // SAFETY: the cell was written once on the boot path and is read-only afterwards.
    let blob = unsafe { &*ACPI.0.get() };
    if blob.len == 0 {
        None
    } else {
        Some(blob.bytes.get(..blob.len).unwrap_or(&[]))
    }
}

fn build_acpi_blob(rsdp_phys: u64, blob: &mut AcpiBlob) -> Result<(), &'static str> {
    if rsdp_phys == 0 {
        return Err("no RSDP");
    }
    let rsdp = phys_slice(rsdp_phys, 36).ok_or("cannot map RSDP")?;
    if rsdp.get(..8).ok_or("short RSDP")? != b"RSD PTR ".as_slice() {
        return Err("bad RSDP signature");
    }
    if !checksum(rsdp.get(..20).ok_or("short RSDP")?) {
        return Err("bad RSDP checksum");
    }
    let revision = *rsdp.get(15).ok_or("short RSDP")?;
    let rsdt_phys = u64::from(read_u32(rsdp, 16).ok_or("short RSDP")?);
    let rsdp_len = if revision >= 2 {
        let length = read_u32(rsdp, 20).ok_or("short RSDP")?;
        if length < 36 {
            return Err("bad RSDP length");
        }
        if !checksum(rsdp) {
            return Err("bad extended RSDP checksum");
        }
        36usize
    } else {
        20usize
    };
    let xsdt_phys = if revision >= 2 {
        read_u64(rsdp, 24).ok_or("short RSDP")?
    } else {
        0
    };

    let mut entries: [BlobEntry; MAX_BLOB_TABLES] = [BlobEntry {
        signature: [0; 8],
        physical: 0,
        length: 0,
    }; MAX_BLOB_TABLES];
    let mut count = 0usize;
    push_entry(
        &mut entries,
        &mut count,
        b"RSD PTR ",
        rsdp_phys,
        u32::try_from(rsdp_len).map_err(|_| "rsdp too large")?,
    )?;
    if xsdt_phys != 0 {
        walk_root_table(&mut entries, &mut count, b"XSDT    ", xsdt_phys, true)?;
    }
    if rsdt_phys != 0 {
        walk_root_table(&mut entries, &mut count, b"RSDT    ", rsdt_phys, false)?;
    }

    let writer: &mut [u8] = &mut blob.bytes;
    writer.fill(0);
    put_u32(writer, 0, ACPI_BLOB_MAGIC).ok_or("short blob")?;
    put_u16(writer, 4, ACPI_BLOB_VERSION).ok_or("short blob")?;
    put_u16(writer, 6, ACPI_BLOB_HEADER_SIZE as u16).ok_or("short blob")?;
    put_u64(writer, 8, rsdp_phys).ok_or("short blob")?;
    put_u32(writer, 16, count as u32).ok_or("short blob")?;
    put_u32(writer, 20, ACPI_BLOB_HEADER_SIZE as u32).ok_or("short blob")?;

    let directory_len = count
        .checked_mul(ACPI_BLOB_ENTRY_SIZE)
        .ok_or("directory overflow")?;
    let payload_start = ACPI_BLOB_HEADER_SIZE
        .checked_add(directory_len)
        .ok_or("directory overflow")?;
    if payload_start > writer.len() {
        return Err("blob too large");
    }
    let mut offset = payload_start;
    let mut index = 0usize;
    while index < count {
        let entry = *entries.get(index).ok_or("missing entry")?;
        let at = ACPI_BLOB_HEADER_SIZE
            .checked_add(
                index
                    .checked_mul(ACPI_BLOB_ENTRY_SIZE)
                    .ok_or("directory overflow")?,
            )
            .ok_or("directory overflow")?;
        let slot_end = at
            .checked_add(ACPI_BLOB_ENTRY_SIZE)
            .ok_or("directory overflow")?;
        let slot = writer.get_mut(at..slot_end).ok_or("directory overflow")?;
        slot.get_mut(0..8)
            .ok_or("directory overflow")?
            .copy_from_slice(&entry.signature);
        put_u32(
            slot,
            8,
            u32::try_from(offset).map_err(|_| "blob too large")?,
        )
        .ok_or("directory overflow")?;
        put_u32(slot, 12, entry.length).ok_or("directory overflow")?;
        slot.get_mut(16..20).ok_or("directory overflow")?.fill(0);
        let end = offset
            .checked_add(entry.length as usize)
            .ok_or("blob overflow")?;
        let destination = writer.get_mut(offset..end).ok_or("blob too large")?;
        let source = phys_slice(entry.physical, entry.length as usize).ok_or("cannot map table")?;
        destination.copy_from_slice(source);
        offset = end;
        index = index.saturating_add(1);
    }
    blob.len = offset;
    Ok(())
}

fn walk_root_table(
    entries: &mut [BlobEntry; MAX_BLOB_TABLES],
    count: &mut usize,
    signature: &[u8; 8],
    physical: u64,
    extended: bool,
) -> Result<(), &'static str> {
    let header = phys_slice(physical, 36).ok_or("cannot map root table")?;
    let expected = signature.get(..4).ok_or("short root table")?;
    if header.get(..4).ok_or("short root table")? != expected {
        return Err("bad root table signature");
    }
    let length = read_u32(header, 4).ok_or("short root table")?;
    if !(36..=MAX_TABLE_BYTES).contains(&length) {
        return Err("bad root table length");
    }
    let bytes = phys_slice(physical, length as usize).ok_or("cannot map root table body")?;
    if !checksum(bytes) {
        return Err("bad root table checksum");
    }
    push_entry(entries, count, signature, physical, length)?;
    let body = bytes.get(36..).ok_or("short root table")?;
    let pointer_size = if extended { 8 } else { 4 };
    let mut index = 0usize;
    while index.saturating_add(pointer_size) <= body.len() {
        let table_phys = if extended {
            read_u64(body, index).ok_or("short root entry")?
        } else {
            u64::from(read_u32(body, index).ok_or("short root entry")?)
        };
        index = index.saturating_add(pointer_size);
        if table_phys == 0 {
            continue;
        }
        walk_sdt(entries, count, table_phys)?;
    }
    Ok(())
}

fn walk_sdt(
    entries: &mut [BlobEntry; MAX_BLOB_TABLES],
    count: &mut usize,
    physical: u64,
) -> Result<(), &'static str> {
    let Some(header) = phys_slice(physical, 36) else {
        return Ok(());
    };
    let signature = header.get(..4).ok_or("short SDT")?;
    // `pcid` only needs MCFG; the firmware DSDT/SSDT blobs are large and not part of the PCI
    // seam, so they are deliberately skipped (the x86-64 capture does the same).
    if signature == b"SSDT" || signature == b"DSDT" {
        return Ok(());
    }
    if !signature
        .iter()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Ok(());
    }
    let length = read_u32(header, 4).ok_or("short SDT")?;
    if !(36..=MAX_TABLE_BYTES).contains(&length) {
        return Ok(());
    }
    let Some(bytes) = phys_slice(physical, length as usize) else {
        return Ok(());
    };
    if !checksum(bytes) {
        return Ok(());
    }
    let mut padded = [b' '; 8];
    padded
        .get_mut(..4)
        .ok_or("short SDT")?
        .copy_from_slice(signature);
    push_entry(entries, count, &padded, physical, length)
}

fn push_entry(
    entries: &mut [BlobEntry; MAX_BLOB_TABLES],
    count: &mut usize,
    signature: &[u8; 8],
    physical: u64,
    length: u32,
) -> Result<(), &'static str> {
    if entries
        .iter()
        .take(*count)
        .any(|entry| entry.physical == physical)
    {
        return Ok(());
    }
    let slot = entries.get_mut(*count).ok_or("too many tables")?;
    slot.signature = *signature;
    slot.physical = physical;
    slot.length = length;
    *count = count.saturating_add(1);
    Ok(())
}

fn phys_slice(physical: u64, length: usize) -> Option<&'static [u8]> {
    if length == 0 {
        return Some(&[]);
    }
    let pointer = crate::memory::frame::phys_to_virt(crate::memory::frame::PhysAddr(physical));
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the HHDM maps every physical region except BadMemory, and firmware ACPI tables are
    // ACPI-reclaimable or reserved memory that the kernel never frees; `length` is the table
    // header's bounded size and the caller only reads the bytes.
    Some(unsafe { core::slice::from_raw_parts(pointer, length) })
}

fn checksum(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) -> Option<()> {
    let end = at.checked_add(2)?;
    bytes
        .get_mut(at..end)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) -> Option<()> {
    let end = at.checked_add(4)?;
    bytes
        .get_mut(at..end)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}

fn put_u64(bytes: &mut [u8], at: usize, value: u64) -> Option<()> {
    let end = at.checked_add(8)?;
    bytes
        .get_mut(at..end)?
        .copy_from_slice(&value.to_le_bytes());
    Some(())
}
