// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 ACPI capture for the `BOOT_BLOB` ACPI bundle.
//!
//! The UEFI loader records the RSDP in `BootInfo.acpi_rsdp`. AAVMF exposes no usable device
//! tree on the QEMU `virt` boot, so `pcid` needs the ACPI MCFG window to enumerate the PCIe
//! root complex and the `virtio-net-pci` function. This module copies the RSDP, the RSDT/XSDT
//! and every referenced table into the same `0x3143_4152` bundle the x86 boot path produces.
//!
//! The capture runs after paging init, when the HHDM covers the ACPI reclaimable pages. Only
//! `LoaderReclaimable` frames are freed, so the ACPI tables remain readable; every range is
//! checked against the published memory map before the read.

#[path = "../../../../crates/raven-acpi/src/imp.rs"]
#[allow(dead_code)] // Shared crate source includes helpers used by its own host tests.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod imp;

use raven_bootinfo::MemoryKind;

use crate::kprintln;
use crate::memory::{frame, paging};

use imp::{
    MAX_BLOB_BYTES, MAX_TABLE_BYTES, MAX_TABLES, RSDP_LENGTH_V2, RSDP_SIGNATURE, TABLE_HEADER,
    TableEntry, parse_rsdp, root_entries, table_entry, table_entry4, table_length,
    table_signature, validate_table, write_blob,
};

struct Early {
    bytes: [u8; MAX_BLOB_BYTES],
    len: usize,
}

struct EarlyCell(core::cell::UnsafeCell<Early>);

// SAFETY: the cell is written once by the boot CPU before user processes exist and is read-only
// afterwards; `capture_early` and `early_blob` run on the boot path.
unsafe impl Sync for EarlyCell {}

static EARLY: EarlyCell = EarlyCell(core::cell::UnsafeCell::new(Early {
    bytes: [0; MAX_BLOB_BYTES],
    len: 0,
}));

/// Capture the ACPI tables into the kernel bundle.
///
/// A missing or invalid RSDP leaves the bundle empty and logs one line; the boot continues so a
/// no-NIC image still reaches login.
pub fn capture_early(rsdp_phys: u64) {
    // SAFETY: this is the single boot-CPU writer; the cell is unpublished until `build` returns.
    let early = unsafe { &mut *EARLY.0.get() };
    match build(rsdp_phys, early) {
        Ok(()) => {}
        Err(reason) => {
            early.len = 0;
            kprintln!("acpi: blob capture unavailable ({reason})");
        }
    }
}

/// The captured ACPI bundle, or `None` when ACPI is absent or the capture failed.
pub fn early_blob() -> Option<&'static [u8]> {
    // SAFETY: the cell was written once during boot and is read-only afterwards.
    let early = unsafe { &*EARLY.0.get() };
    let bytes = early.bytes.get(..early.len)?;
    (!bytes.is_empty()).then_some(bytes)
}

fn build(rsdp_phys: u64, early: &mut Early) -> Result<(), &'static str> {
    if rsdp_phys == 0 {
        return Err("no RSDP");
    }
    let rsdp_bytes = physical(rsdp_phys, RSDP_LENGTH_V2).ok_or("RSDP not mapped")?;
    let rsdp = parse_rsdp(rsdp_bytes).map_err(|_| "bad RSDP")?;

    let mut tables = [TableEntry::EMPTY; MAX_TABLES];
    let rsdp_entry = tables.first_mut().ok_or("empty table array")?;
    *rsdp_entry = table_entry(RSDP_SIGNATURE, rsdp_phys, u32::from(rsdp.length));
    let mut count = 1usize;

    let (root_physical, extended) = if rsdp.xsdt != 0 {
        (rsdp.xsdt, true)
    } else if rsdp.rsdt != 0 {
        (u64::from(rsdp.rsdt), false)
    } else {
        return Err("no RSDT/XSDT");
    };
    let root_header = physical(root_physical, TABLE_HEADER).ok_or("root table not mapped")?;
    let root_length = table_length(root_header).ok_or("root table length")?;
    let root_length = usize::try_from(root_length).map_err(|_| "root table length")?;
    if root_length < TABLE_HEADER || root_length > MAX_TABLE_BYTES as usize {
        return Err("root table length");
    }
    let root = physical(root_physical, root_length).ok_or("root table not mapped")?;
    let expected: &[u8; 4] = if extended { b"XSDT" } else { b"RSDT" };
    if table_signature(root).as_ref() != Some(expected) {
        return Err("root signature");
    }
    let entries = root_entries(root, extended).map_err(|_| "bad root table")?;
    for address in entries {
        if count >= MAX_TABLES {
            return Err("too many tables");
        }
        let header = physical(address, TABLE_HEADER).ok_or("table not mapped")?;
        let length = table_length(header).ok_or("table length")?;
        if length < TABLE_HEADER as u32 || length > MAX_TABLE_BYTES {
            return Err("table length");
        }
        let length_usize = usize::try_from(length).map_err(|_| "table length")?;
        let bytes = physical(address, length_usize).ok_or("table not mapped")?;
        let signature = table_signature(bytes).ok_or("table signature")?;
        validate_table(bytes, &signature).map_err(|_| "bad table checksum")?;
        let slot = tables.get_mut(count).ok_or("too many tables")?;
        *slot = table_entry4(&signature, address, length);
        count = count.saturating_add(1);
    }

    let bundle = tables.get(..count).ok_or("too many tables")?;
    let written = write_blob(&mut early.bytes, bundle, |entry, destination| {
        match physical(entry.physical, destination.len()) {
            Some(source) => {
                destination.copy_from_slice(source);
                true
            }
            None => false,
        }
    })
    .map_err(|_| "bundle too large")?;
    early.len = written;
    Ok(())
}

/// A read-only view of one physical range through the HHDM, after checking the memory map.
fn physical(address: u64, length: usize) -> Option<&'static [u8]> {
    if length == 0 {
        return None;
    }
    let length = u64::try_from(length).ok()?;
    let end = address.checked_add(length)?;
    let mapped = paging::memory_map().iter().any(|region| {
        region.kind() != Some(MemoryKind::BadMemory)
            && region.base <= address
            && end <= region.base.saturating_add(region.len)
    });
    if !mapped {
        return None;
    }
    let pointer = frame::phys_to_virt(frame::PhysAddr(address));
    if pointer.is_null() {
        return None;
    }
    let length = usize::try_from(length).ok()?;
    // SAFETY: the memory map covers the range and the HHDM mapping is live for the kernel's
    // lifetime; the returned slice is read-only and the caller copies it before any reuse.
    Some(unsafe { core::slice::from_raw_parts(pointer, length) })
}
