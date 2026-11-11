// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ACPI MADT discovery.
//!
//! The `acpi` crate locates and validates the RSDP/RSDT/XSDT and the MADT header. The variable
//! MADT entry list is parsed here with explicit bounds checks, because the entry lengths are
//! firmware-controlled and must not be trusted.
//!
//! The `acpi` crate's `platform::InterruptModel` helper is behind its nightly `allocator_api`
//! feature, so this module parses the stable MADT entry types directly.

use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr::NonNull;

use acpi::madt::Madt;
use acpi::mcfg::Mcfg;
use acpi::{AcpiHandler, AcpiTables, PhysicalMapping};
use raven_bootinfo::MemoryKind;

use crate::kprintln;
use crate::memory::{frame, mmio, paging};

/// Magic of the kernel's ACPI bundle (`"RAC1"`).
pub const ACPI_BLOB_MAGIC: u32 = 0x3143_4152;
/// ACPI bundle layout version.
pub const ACPI_BLOB_VERSION: u16 = 1;
/// Size of the ACPI bundle header in bytes.
pub const ACPI_BLOB_HEADER_SIZE: u32 = 24;
/// Size of one ACPI bundle directory entry in bytes.
pub const ACPI_BLOB_ENTRY_SIZE: u32 = 20;

/// Maximum local APIC ids retained for S10.
const MAX_CPUS: usize = 64;
/// Maximum IOAPICs retained.
const MAX_IOAPICS: usize = 8;
/// Maximum interrupt source overrides retained.
const MAX_OVERRIDES: usize = 32;
/// xAPIC base used when firmware provides no MADT.
const DEFAULT_LOCAL_APIC: u64 = 0xFEE0_0000;
/// IOAPIC base used when firmware provides no MADT.
const DEFAULT_IOAPIC: u64 = 0xFEC0_0000;

/// One IOAPIC from the MADT.
#[allow(dead_code)] // `id` is reported to drivers by S07.
#[derive(Clone, Copy, Debug)]
pub struct IoApicInfo {
    /// IOAPIC id.
    pub id: u8,
    /// Physical register base.
    pub address: u64,
    /// First global system interrupt served by this IOAPIC.
    pub gsi_base: u32,
}

const DEFAULT_IOAPIC_INFO: IoApicInfo = IoApicInfo {
    id: 0,
    address: DEFAULT_IOAPIC,
    gsi_base: 0,
};

/// One ISA interrupt-source override.
#[allow(dead_code)] // Fields are consumed by ioapic::route and S07 drivers.
#[derive(Clone, Copy, Debug)]
pub struct Override {
    /// Source bus (0 is ISA).
    pub bus: u8,
    /// Bus-relative IRQ.
    pub irq: u8,
    /// Global system interrupt.
    pub gsi: u32,
    /// Line is active-low.
    pub active_low: bool,
    /// Line is level-triggered.
    pub level: bool,
}

const NO_OVERRIDE: Override = Override {
    bus: 0,
    irq: 0,
    gsi: 0,
    active_low: false,
    level: false,
};

/// Parsed platform interrupt information.
#[derive(Clone, Copy, Debug)]
pub struct AcpiInfo {
    /// True when a valid MADT was parsed.
    pub available: bool,
    /// Number of enabled CPUs.
    pub cpus: u32,
    /// Local APIC ids, in MADT order.
    pub apic_ids: [u32; MAX_CPUS],
    /// First CPU's local APIC id.
    pub boot_apic_id: u32,
    /// Local APIC MMIO base.
    pub local_apic_address: u64,
    /// IOAPICs.
    pub io_apics: [IoApicInfo; MAX_IOAPICS],
    /// Number of valid IOAPICs.
    pub io_apic_count: usize,
    /// ISA overrides.
    pub overrides: [Override; MAX_OVERRIDES],
    /// Number of valid overrides.
    pub override_count: usize,
    /// Firmware says the legacy 8259 PIC is present.
    pub legacy_pics: bool,
    /// A valid MCFG was parsed.
    pub mcfg_available: bool,
    /// ECAM base address from the first MCFG entry.
    pub mcfg_base: u64,
    /// First bus of the first MCFG entry.
    pub mcfg_start_bus: u8,
    /// Last bus of the first MCFG entry.
    pub mcfg_end_bus: u8,
}

impl AcpiInfo {
    const fn empty() -> Self {
        Self {
            available: false,
            cpus: 1,
            apic_ids: [0; MAX_CPUS],
            boot_apic_id: 0,
            local_apic_address: DEFAULT_LOCAL_APIC,
            io_apics: [DEFAULT_IOAPIC_INFO; MAX_IOAPICS],
            io_apic_count: 1,
            overrides: [NO_OVERRIDE; MAX_OVERRIDES],
            override_count: 0,
            legacy_pics: false,
            mcfg_available: false,
            mcfg_base: 0,
            mcfg_start_bus: 0,
            mcfg_end_bus: 0,
        }
    }

    /// Translate an ISA IRQ to its GSI through the MADT overrides.
    #[allow(dead_code)] // S07/S21 route ISA IRQs through this helper.
    pub fn gsi_for_isa(&self, irq: u8) -> u32 {
        let mut index = 0usize;
        while index < self.override_count {
            if let Some(entry) = self.overrides.get(index)
                && entry.bus == 0
                && entry.irq == irq
            {
                return entry.gsi;
            }
            index = index.saturating_add(1);
        }
        u32::from(irq)
    }

    /// Polarity and trigger flags for an ISA IRQ override.
    #[allow(dead_code)] // S07/S21 route ISA IRQs through this helper.
    pub fn override_for_isa(&self, irq: u8) -> Option<(bool, bool)> {
        let mut index = 0usize;
        while index < self.override_count {
            if let Some(entry) = self.overrides.get(index)
                && entry.bus == 0
                && entry.irq == irq
            {
                return Some((entry.active_low, entry.level));
            }
            index = index.saturating_add(1);
        }
        None
    }
}

struct InfoCell(UnsafeCell<AcpiInfo>);

// SAFETY: the cell is written once during single-CPU init and read-only afterwards.
unsafe impl Sync for InfoCell {}

static INFO: InfoCell = InfoCell(UnsafeCell::new(AcpiInfo::empty()));

/// Parsed ACPI information.
pub fn info() -> &'static AcpiInfo {
    // SAFETY: the cell is written once during init and only read afterwards.
    unsafe { &*INFO.0.get() }
}

/// Parse the MADT and log the platform line.
pub fn init() {
    // SAFETY: paging published the copied BootInfo before ACPI init runs.
    let boot = unsafe { &*paging::boot_info_ptr() };
    if boot.acpi_rsdp == 0 {
        store(AcpiInfo::empty());
        kprintln!("acpi: unavailable");
        return;
    }

    match parse(boot.acpi_rsdp as usize) {
        Ok(parsed) => {
            if parsed.mcfg_available {
                kprintln!(
                    "acpi: mcfg base={:#x} buses={}-{}",
                    parsed.mcfg_base,
                    parsed.mcfg_start_bus,
                    parsed.mcfg_end_bus
                );
            }
            store(parsed);
            kprintln!(
                "acpi: {} cpus, {} ioapics, {} overrides",
                parsed.cpus,
                parsed.io_apic_count,
                parsed.override_count
            );
        }
        Err(()) => {
            store(AcpiInfo::empty());
            kprintln!("acpi: unavailable");
        }
    }
}

fn store(info: AcpiInfo) {
    // SAFETY: the cell is written once during single-CPU init before it is published.
    unsafe {
        INFO.0.get().write(info);
    }
}

#[derive(Clone, Copy)]
struct AcpiMapper;

impl AcpiHandler for AcpiMapper {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let virtual_address = map_physical(physical_address as u64, size);
        let Some(pointer) = NonNull::new(virtual_address.cast::<T>()) else {
            kprintln!("acpi: cannot map table at {physical_address:#x}");
            crate::boot::exit_qemu(0x11);
        };
        // SAFETY: the caller guarantees the physical range is valid; the mapping covers `size`
        // bytes and the returned pointer is non-null.
        unsafe { PhysicalMapping::new(physical_address, pointer, size, size, *self) }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {
        // HHDM and the MMIO bump window are permanent kernel mappings.
    }
}

fn map_physical(physical: u64, size: usize) -> *mut u8 {
    if size == 0 {
        return NonNull::<u8>::dangling().as_ptr();
    }
    if is_ram_range(physical, size) {
        let pointer = frame::phys_to_virt(frame::PhysAddr(physical));
        if !pointer.is_null() {
            return pointer;
        }
    }
    match mmio::map_mmio(physical, size) {
        Ok(pointer) => pointer,
        Err(error) => {
            kprintln!("acpi: mmio map failed: {error}");
            crate::boot::exit_qemu(0x11);
        }
    }
}

fn is_ram_range(physical: u64, size: usize) -> bool {
    let Some(end) = physical.checked_add(size as u64) else {
        return false;
    };
    paging::memory_map().iter().any(|region| {
        let kind = region.kind();
        kind != Some(MemoryKind::Mmio)
            && kind != Some(MemoryKind::BadMemory)
            && physical >= region.base
            && region.end().is_some_and(|region_end| end <= region_end)
    })
}

fn parse(rsdp: usize) -> Result<AcpiInfo, ()> {
    // SAFETY: `rsdp` comes from the validated BootInfo; the handler bounds-checks mappings.
    let tables = unsafe { AcpiTables::from_rsdp(AcpiMapper, rsdp) }.map_err(|_| ())?;
    let madt = tables.find_table::<Madt>().map_err(|_| ())?;
    let madt_pointer = madt.virtual_start().as_ptr();
    // SAFETY: `madt_pointer` is the start of the mapped MADT; the header is copied out because
    // the packed structure has no aligned reference.
    let header = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*madt_pointer).header)) };
    let total = header.length as usize;
    if total < size_of::<Madt>() || total > madt.region_length() {
        return Err(());
    }
    // SAFETY: same mapped MADT; copy the packed fields by value.
    let local_apic_address = unsafe {
        core::ptr::read_unaligned(core::ptr::addr_of!((*madt_pointer).local_apic_address))
    };
    // SAFETY: same mapped MADT; copy the packed flag field by value.
    let madt_flags =
        unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*madt_pointer).flags)) };

    // SAFETY: the acpi crate mapped and validated `total` bytes at this address.
    let bytes = unsafe { core::slice::from_raw_parts(madt_pointer.cast::<u8>(), total) };
    let entries = bytes.get(size_of::<Madt>()..).ok_or(())?;

    let mut info = AcpiInfo::empty();
    info.available = true;
    if let Ok(mcfg) = tables.find_table::<Mcfg>()
        && let Some(entry) = mcfg.entries().first()
    {
        info.mcfg_available = true;
        info.mcfg_base = entry.base_address;
        info.mcfg_start_bus = entry.bus_number_start;
        info.mcfg_end_bus = entry.bus_number_end;
    }
    info.cpus = 0;
    info.io_apic_count = 0;
    info.local_apic_address = if local_apic_address == 0 {
        DEFAULT_LOCAL_APIC
    } else {
        u64::from(local_apic_address)
    };
    info.legacy_pics = madt_flags & 1 != 0;
    info.boot_apic_id = 0;

    let mut offset = 0usize;
    while offset.saturating_add(2) <= entries.len() {
        let entry_type = *entries.get(offset).ok_or(())?;
        let length = usize::from(*entries.get(offset.saturating_add(1)).ok_or(())?);
        if length < 2 || offset.saturating_add(length) > entries.len() {
            return Err(());
        }
        let body = entries
            .get(offset..offset.saturating_add(length))
            .ok_or(())?;
        match entry_type {
            0 => {
                if length >= 8 {
                    let apic_id = *body.get(3).ok_or(())?;
                    let flags = read_u32(body, 4).ok_or(())?;
                    if flags & 1 != 0 {
                        push_apic_id(&mut info, u32::from(apic_id));
                    }
                }
            }
            9 => {
                if length >= 16 {
                    let apic_id = read_u32(body, 4).ok_or(())?;
                    let flags = read_u32(body, 8).ok_or(())?;
                    if flags & 1 != 0 {
                        push_apic_id(&mut info, apic_id);
                    }
                }
            }
            1 => {
                if length >= 12 {
                    let id = *body.get(2).ok_or(())?;
                    let address = u64::from(read_u32(body, 4).ok_or(())?);
                    let gsi_base = read_u32(body, 8).ok_or(())?;
                    if info.io_apic_count < MAX_IOAPICS
                        && let Some(slot) = info.io_apics.get_mut(info.io_apic_count)
                    {
                        *slot = IoApicInfo {
                            id,
                            address,
                            gsi_base,
                        };
                        info.io_apic_count = info.io_apic_count.saturating_add(1);
                    }
                }
            }
            2 => {
                if length >= 10 {
                    let bus = *body.get(2).ok_or(())?;
                    let irq = *body.get(3).ok_or(())?;
                    let gsi = read_u32(body, 4).ok_or(())?;
                    let flags = read_u16(body, 8).ok_or(())?;
                    if info.override_count < MAX_OVERRIDES
                        && let Some(slot) = info.overrides.get_mut(info.override_count)
                    {
                        *slot = Override {
                            bus,
                            irq,
                            gsi,
                            active_low: flags & 0x3 == 0x3,
                            level: (flags >> 2) & 0x3 == 0x3,
                        };
                        info.override_count = info.override_count.saturating_add(1);
                    }
                }
            }
            5 => {
                if length >= 12
                    && let Some(address) = read_u64(body, 4)
                    && address != 0
                {
                    info.local_apic_address = address;
                }
            }
            _ => {}
        }
        offset = offset.saturating_add(length);
    }

    if info.cpus == 0 {
        info.cpus = 1;
        info.apic_ids[0] = 0;
    }
    info.boot_apic_id = info.apic_ids[0];
    if info.io_apic_count == 0 {
        info.io_apics[0] = DEFAULT_IOAPIC_INFO;
        info.io_apic_count = 1;
    }

    Ok(info)
}

fn push_apic_id(info: &mut AcpiInfo, apic_id: u32) {
    if info.cpus < MAX_CPUS as u32
        && let Some(slot) = info.apic_ids.get_mut(info.cpus as usize)
    {
        *slot = apic_id;
        info.cpus = info.cpus.saturating_add(1);
    }
}

/// Upper bound of the captured ACPI bundle, including header, directory and table copies.
const MAX_EARLY_BLOB: usize = 256 * 1024;
/// Largest ACPI table copied into the bundle.
const MAX_TABLE_BYTES: u32 = 64 * 1024;
/// Largest number of bundle entries (RSDP, root tables and SDTs).
const MAX_BLOB_TABLES: usize = 64;

struct EarlyBlob {
    bytes: [u8; MAX_EARLY_BLOB],
    len: usize,
}

struct EarlyCell(core::cell::UnsafeCell<EarlyBlob>);

// SAFETY: the cell is written once during single-CPU paging init, before loader memory is
// reclaimed, and is read-only afterwards.
unsafe impl Sync for EarlyCell {}

static EARLY: EarlyCell = EarlyCell(core::cell::UnsafeCell::new(EarlyBlob {
    bytes: [0; MAX_EARLY_BLOB],
    len: 0,
}));

/// One bundle entry collected before the header is written.
#[derive(Clone, Copy)]
struct BlobEntry {
    signature: [u8; 8],
    physical: u64,
    length: u32,
}

/// Capture the ACPI tables into kernel memory while loader memory is still mapped.
///
/// Called from `paging::init` on x86-64 before `LoaderReclaimable` frames are freed, so a
/// firmware that places the RSDP or its tables in loader-owned memory keeps them reachable here.
pub fn capture_early(rsdp_phys: u64) {
    // SAFETY: this is the single-CPU init writer; the cell is unpublished until it is complete.
    let early = unsafe { &mut *EARLY.0.get() };
    match build_early_blob(rsdp_phys, early) {
        Ok(()) => {}
        Err(reason) => {
            early.len = 0;
            kprintln!("acpi: blob capture unavailable ({reason})");
        }
    }
}

/// The captured ACPI bundle, or `None` when the capture failed or ACPI is absent.
pub fn early_blob() -> Option<&'static [u8]> {
    // SAFETY: the cell was written once during init and is read-only afterwards.
    let early = unsafe { &*EARLY.0.get() };
    if early.len == 0 {
        None
    } else {
        Some(early.bytes.get(..early.len).unwrap_or(&[]))
    }
}

fn build_early_blob(rsdp_phys: u64, early: &mut EarlyBlob) -> Result<(), &'static str> {
    if rsdp_phys == 0 {
        return Err("no RSDP");
    }
    let rsdp_pointer = map_physical(rsdp_phys, 36);
    if rsdp_pointer.is_null() {
        return Err("cannot map RSDP");
    }
    // SAFETY: `map_physical` mapped 36 readable bytes at the pointer.
    let rsdp = unsafe { core::slice::from_raw_parts(rsdp_pointer, 36) };
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

    // Header: magic, version, header size, rsdp_phys, table count, directory offset.
    let writer: &mut [u8] = &mut early.bytes;
    writer.fill(0);
    put_u32(writer, 0, ACPI_BLOB_MAGIC).ok_or("short blob")?;
    put_u16(writer, 4, ACPI_BLOB_VERSION).ok_or("short blob")?;
    put_u16(writer, 6, ACPI_BLOB_HEADER_SIZE as u16).ok_or("short blob")?;
    put_u64(writer, 8, rsdp_phys).ok_or("short blob")?;
    put_u32(writer, 16, count as u32).ok_or("short blob")?;
    put_u32(writer, 20, ACPI_BLOB_HEADER_SIZE).ok_or("short blob")?;

    let directory_len = count
        .checked_mul(ACPI_BLOB_ENTRY_SIZE as usize)
        .ok_or("directory overflow")?;
    let payload_start = (ACPI_BLOB_HEADER_SIZE as usize)
        .checked_add(directory_len)
        .ok_or("directory overflow")?;
    if payload_start > writer.len() {
        return Err("blob too large");
    }
    let mut offset = payload_start;
    for (index, entry) in entries.iter().take(count).enumerate() {
        let at = (ACPI_BLOB_HEADER_SIZE as usize)
            .checked_add(index.saturating_mul(ACPI_BLOB_ENTRY_SIZE as usize))
            .ok_or("directory overflow")?;
        let slot_end = at
            .checked_add(ACPI_BLOB_ENTRY_SIZE as usize)
            .ok_or("directory overflow")?;
        let slot = writer.get_mut(at..slot_end).ok_or("directory overflow")?;
        slot.get_mut(0..8)
            .ok_or("directory overflow")?
            .copy_from_slice(&entry.signature);
        put_u32(slot, 8, offset as u32).ok_or("directory overflow")?;
        put_u32(slot, 12, entry.length).ok_or("directory overflow")?;
        slot.get_mut(16..20).ok_or("directory overflow")?.fill(0);
        let end = offset
            .checked_add(entry.length as usize)
            .ok_or("blob overflow")?;
        let destination = writer.get_mut(offset..end).ok_or("blob too large")?;
        let source_pointer = map_physical(entry.physical, entry.length as usize);
        if source_pointer.is_null() {
            return Err("cannot map table");
        }
        // SAFETY: `map_physical` mapped `length` readable bytes at the pointer.
        let source = unsafe { core::slice::from_raw_parts(source_pointer, entry.length as usize) };
        destination.copy_from_slice(source);
        offset = end;
    }
    early.len = offset;
    Ok(())
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

fn walk_root_table(
    entries: &mut [BlobEntry; MAX_BLOB_TABLES],
    count: &mut usize,
    signature: &[u8; 8],
    physical: u64,
    extended: bool,
) -> Result<(), &'static str> {
    let header_pointer = map_physical(physical, 36);
    if header_pointer.is_null() {
        return Err("cannot map root table");
    }
    // SAFETY: `map_physical` mapped 36 readable bytes at the pointer.
    let header = unsafe { core::slice::from_raw_parts(header_pointer, 36) };
    let expected = signature.get(..4).ok_or("short root table")?;
    if header.get(..4).ok_or("short root table")? != expected {
        return Err("bad root table signature");
    }
    let length = read_u32(header, 4).ok_or("short root table")?;
    if !(36..=MAX_TABLE_BYTES).contains(&length) {
        return Err("bad root table length");
    }
    let bytes_pointer = map_physical(physical, length as usize);
    if bytes_pointer.is_null() {
        return Err("cannot map root table body");
    }
    // SAFETY: `map_physical` mapped `length` readable bytes at the pointer.
    let bytes = unsafe { core::slice::from_raw_parts(bytes_pointer, length as usize) };
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
    let header_pointer = map_physical(physical, 36);
    if header_pointer.is_null() {
        return Ok(());
    }
    // SAFETY: `map_physical` mapped 36 readable bytes at the pointer.
    let header = unsafe { core::slice::from_raw_parts(header_pointer, 36) };
    let signature = header.get(..4).ok_or("short SDT")?;
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
    let bytes_pointer = map_physical(physical, length as usize);
    if bytes_pointer.is_null() {
        return Ok(());
    }
    // SAFETY: `map_physical` mapped `length` readable bytes at the pointer.
    let bytes = unsafe { core::slice::from_raw_parts(bytes_pointer, length as usize) };
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

fn checksum(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let slice = bytes.get(offset..end)?;
    Some(u16::from_le_bytes([*slice.first()?, *slice.get(1)?]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let slice = bytes.get(offset..end)?;
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

const _: () = {
    assert!(MAX_CPUS > 0);
    assert!(MAX_IOAPICS > 0);
    assert!(MAX_OVERRIDES > 0);
};
