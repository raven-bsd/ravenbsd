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
use acpi::{AcpiHandler, AcpiTables, PhysicalMapping};
use raven_bootinfo::MemoryKind;

use crate::kprintln;
use crate::memory::{frame, mmio, paging};

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
