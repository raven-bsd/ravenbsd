// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Intel VT-d: DMAR discovery, translation tables and fault reporting.
//!
//! The unit is enabled once during boot from the ACPI DMAR table captured by
//! the early ACPI path. Each device gets a second-level domain with its own
//! IOVA allocator; page tables are built lazily when a DMA buffer is allocated.
//! When the platform has no usable DMAR, or the table is malformed, the unit is
//! not enabled and `driver::DmaDomain` stays the explicit identity/unprotected
//! fallback.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use raven_iommu::{
    DmarTable, Drhd, IovaAllocator, PAGE_SIZE, TABLE_ENTRIES, context_entry, decode_fault,
    fault_reason_text, page_entry, root_entry, table_index,
};
use spin::Mutex;

use crate::arch::acpi;
use crate::kprintln;
use crate::memory::frame;
use crate::memory::mmio;

/// VT-d register offsets.
const REG_VERSION: usize = 0x000;
const REG_CAP: usize = 0x008;
const REG_ECAP: usize = 0x010;
const REG_GCMD: usize = 0x018;
const REG_GSTS: usize = 0x01C;
const REG_RTADDR: usize = 0x020;
const REG_CCMD: usize = 0x028;
const REG_FSTS: usize = 0x034;

const GCMD_TE: u32 = 1 << 31;
const GCMD_SRTP: u32 = 1 << 30;
const GSTS_TES: u32 = 1 << 31;
const GSTS_RTPS: u32 = 1 << 30;
const CCMD_ICC: u64 = 1 << 63;
const CCMD_CIRG_GLOBAL: u64 = 1 << 61;
const CCMD_CAIG_GLOBAL: u64 = 1 << 59;
const IOTLB_REG_OFF: usize = 0x8;
const IOTLB_IVT: u64 = 1 << 63;
const IOTLB_IIRG_GLOBAL: u64 = 1 << 60;
const IOTLB_IAIG_GLOBAL: u64 = 1 << 57;
const IOTLB_DRAIN_READ: u64 = 1 << 49;
const IOTLB_DRAIN_WRITE: u64 = 1 << 48;
const FSTS_PPF: u32 = 1 << 1;
const FSTS_PFO: u32 = 1;
const FSTS_CLEAR: u32 = 0x7F;

const CAP_SAGAW_SHIFT: u32 = 8;
const CAP_ND_SHIFT: u32 = 0;
const CAP_NFR_SHIFT: u32 = 40;
const CAP_FRO_SHIFT: u32 = 24;
const ECAP_PT: u64 = 1 << 6;

/// One page of root/context/page-table storage.
const TABLE_BYTES: usize = PAGE_SIZE as usize;
/// Bounded wait for a hardware completion bit.
const WAIT_LOOPS: usize = 1_000_000;

/// A PCI bus/device/function identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bdf {
    /// PCI bus.
    pub bus: u8,
    /// PCI device (slot).
    pub device: u8,
    /// PCI function.
    pub function: u8,
}

impl Bdf {
    /// VT-d source id (`bus << 8 | device << 3 | function`).
    pub fn source_id(self) -> u16 {
        (u16::from(self.bus) << 8)
            | (u16::from(self.device & 0x1F) << 3)
            | u16::from(self.function & 0x7)
    }

    /// `bb:dd.f` text for logs.
    pub fn text(self) -> alloc::string::String {
        alloc::format!("{:02x}:{:02x}.{}", self.bus, self.device, self.function)
    }
}

/// One protected device domain.
pub struct Domain {
    id: u16,
    bdf: Bdf,
    levels: u8,
    root: u64,
    iova: Mutex<IovaAllocator>,
    tables: Mutex<Vec<u64>>,
}

impl Domain {
    /// Domain identifier reported by `DMA_INFO`.
    pub fn id(&self) -> u16 {
        self.id
    }

    /// First IOVA managed by the domain.
    pub fn iova_start(&self) -> u64 {
        self.iova.lock().start()
    }

    /// One past the last IOVA managed by the domain.
    pub fn iova_end(&self) -> u64 {
        self.iova.lock().end()
    }

    /// Allocate an IOVA range and map `frames` at it.
    pub fn map(
        &self,
        unit: &Unit,
        frames: &[u64],
        len: u64,
        align: u64,
        limit: u64,
    ) -> Result<u64, &'static str> {
        let pages = len / PAGE_SIZE;
        if pages == 0 || frames.len() < usize::try_from(pages).map_err(|_| "too many pages")? {
            return Err("invalid mapping length");
        }
        let iova = self
            .iova
            .lock()
            .allocate_within(len, align, limit)
            .ok_or("IOVA space exhausted")?;
        if let Err(error) = self.map_pages(unit, iova, frames, pages) {
            let _ = self.unmap_pages(iova, len);
            let _ = self.iova.lock().release(iova, len);
            return Err(error);
        }
        unit.invalidate_iotlb_global()?;
        Ok(iova)
    }

    /// Remove a mapping and release its IOVA range.
    pub fn unmap(&self, unit: &Unit, iova: u64, len: u64) {
        let _ = self.unmap_pages(iova, len);
        let _ = self.iova.lock().release(iova, len);
        let _ = unit.invalidate_iotlb_global();
    }

    fn map_pages(
        &self,
        unit: &Unit,
        iova: u64,
        frames: &[u64],
        pages: u64,
    ) -> Result<(), &'static str> {
        let mut tables = self.tables.lock();
        let mut page = 0u64;
        while page < pages {
            let address = iova
                .checked_add(page.saturating_mul(PAGE_SIZE))
                .ok_or("IOVA overflow")?;
            let physical = *frames
                .get(usize::try_from(page).map_err(|_| "page overflow")?)
                .ok_or("missing backing frame")?;
            self.set_leaf(&mut tables, unit, address, physical)?;
            page = page.saturating_add(1);
        }
        Ok(())
    }

    fn set_leaf(
        &self,
        tables: &mut Vec<u64>,
        _unit: &Unit,
        address: u64,
        physical: u64,
    ) -> Result<(), &'static str> {
        let mut table = self.root;
        let mut level = 0u8;
        while level < self.levels.saturating_sub(1) {
            let index = table_index(self.levels, level, address);
            let slot = table_slot(table, index)?;
            // SAFETY: `table` is a live zeroed page table frame and `index` is bounded by
            // `table_slot`; the entry is owned by this domain's `tables` lock.
            let entry = unsafe { core::ptr::read_volatile(slot) };
            if entry & 1 == 0 {
                let child = frame::alloc_zeroed_frame().ok_or("no page-table frame")?;
                tables.push(child.0);
                // SAFETY: same live page table; publish the child before it is used by the IOMMU.
                unsafe {
                    core::ptr::write_volatile(slot, page_entry(child.0, true));
                }
                table = child.0;
            } else {
                table = entry & raven_iommu::PTE_ADDRESS_MASK;
            }
            level = level.saturating_add(1);
        }
        let index = table_index(self.levels, self.levels.saturating_sub(1), address);
        let slot = table_slot(table, index)?;
        // SAFETY: same live page table and bounded index.
        unsafe {
            core::ptr::write_volatile(slot, page_entry(physical, true));
        }
        Ok(())
    }

    fn unmap_pages(&self, iova: u64, len: u64) -> Result<(), &'static str> {
        let pages = len / PAGE_SIZE;
        let _tables = self.tables.lock();
        let mut page = 0u64;
        while page < pages {
            let address = iova
                .checked_add(page.saturating_mul(PAGE_SIZE))
                .ok_or("IOVA overflow")?;
            let mut table = self.root;
            let mut level = 0u8;
            let mut present = true;
            while level < self.levels.saturating_sub(1) {
                let index = table_index(self.levels, level, address);
                let slot = table_slot(table, index)?;
                // SAFETY: live page table, bounded index.
                let entry = unsafe { core::ptr::read_volatile(slot) };
                if entry & 1 == 0 {
                    present = false;
                    break;
                }
                table = entry & raven_iommu::PTE_ADDRESS_MASK;
                level = level.saturating_add(1);
            }
            if present {
                let index = table_index(self.levels, self.levels.saturating_sub(1), address);
                let slot = table_slot(table, index)?;
                // SAFETY: live page table, bounded index.
                unsafe {
                    core::ptr::write_volatile(slot, 0);
                }
            }
            page = page.saturating_add(1);
        }
        Ok(())
    }
}

/// One enabled VT-d unit.
pub struct Unit {
    base: usize,
    phys: u64,
    cap: u64,
    ecap: u64,
    iotlb: usize,
    haw: u8,
    flags: u8,
    rmrr_count: usize,
    atsr_count: usize,
    scope_count: usize,
    root: u64,
    levels: u8,
    next_domain: AtomicU16,
    context_tables: Mutex<BTreeMap<u8, u64>>,
    domains: Mutex<BTreeMap<u16, Arc<Domain>>>,
    by_sid: Mutex<BTreeMap<u16, u16>>,
    table_frames: Mutex<Vec<u64>>,
    started: AtomicBool,
}

static UNIT: Mutex<Option<Arc<Unit>>> = Mutex::new(None);

/// Whether a VT-d unit is enabled and usable.
pub fn available() -> bool {
    UNIT.lock().is_some()
}

/// The enabled unit, if any.
fn unit() -> Option<Arc<Unit>> {
    UNIT.lock().clone()
}

/// Initialise VT-d from the captured ACPI bundle. Never panics; a failure
/// leaves the machine on the identity/unprotected DMA path.
///
/// Translation is opt-in with the boot parameter `iommu=on`. Drivers that do
/// not negotiate a platform IOMMU (notably the current virtio drivers) keep
/// working when the firmware advertises a DMAR but the operator has not opted
/// in yet.
pub fn init() {
    let cmdline = crate::cmdline::CmdLine::parse(crate::memory::paging::cmdline());
    if cmdline.get("iommu") != Some("on") {
        kprintln!("vtd: disabled (iommu=on not set); DMA unprotected");
        return;
    }
    match try_init() {
        Ok(unit) => {
            kprintln!(
                "vtd: dmar units=1 drhd={:#x} haw={} levels={} flags={:#x} rmrr={} atsr={} scopes={} cap={:#x} ecap={:#x}",
                unit.phys,
                unit.haw,
                unit.levels,
                unit.flags,
                unit.rmrr_count,
                unit.atsr_count,
                unit.scope_count,
                unit.cap,
                unit.ecap
            );
            *UNIT.lock() = Some(Arc::new(unit));
        }
        Err(reason) => {
            kprintln!("vtd: {reason}; DMA unprotected");
        }
    }
}

fn try_init() -> Result<Unit, &'static str> {
    let bytes = find_dmar().ok_or("no DMAR table")?;
    let table = DmarTable::parse(bytes).map_err(|error| match error {
        raven_iommu::DmarError::BadSignature => "bad DMAR signature",
        raven_iommu::DmarError::BadChecksum => "bad DMAR checksum",
        raven_iommu::DmarError::BadLength => "bad DMAR structure length",
        raven_iommu::DmarError::BadUnit => "bad DRHD base or segment",
        raven_iommu::DmarError::NoUnits => "DMAR has no DRHD",
        raven_iommu::DmarError::TooShort => "DMAR is too short",
    })?;
    if table.units.len() != 1 {
        return Err("multiple DRHD units are not supported");
    }
    if !table.rmrr.is_empty() {
        return Err("RMRR is not supported");
    }
    let drhd: &Drhd = table.first_unit();
    // QEMU describes the covered functions explicitly instead of setting
    // INCLUDE_PCI_ALL. Both forms are valid; the context install is per BDF and
    // the test only claims isolation for devices that have a context entry.
    let _ = (drhd.include_all, drhd.scopes.len());

    let pointer =
        mmio::map_mmio(drhd.base, TABLE_BYTES).map_err(|_| "cannot map DMAR registers")?;
    let base = pointer as usize;
    let version = read32(base, REG_VERSION);
    if version >> 4 != 1 {
        return Err("unsupported VT-d version");
    }
    let cap = read64(base, REG_CAP);
    let ecap = read64(base, REG_ECAP);
    let sagaw = ((cap >> CAP_SAGAW_SHIFT) & 0x1F) as u8;
    let iotlb = base.saturating_add(16usize.saturating_mul(((ecap >> 8) & 0x3FF) as usize));
    let levels = if sagaw & 0x04 != 0 {
        4
    } else if sagaw & 0x02 != 0 {
        3
    } else if sagaw & 0x01 != 0 {
        2
    } else {
        return Err("no supported AGAW");
    };
    let nd = ((cap >> CAP_ND_SHIFT) & 0x7) as u32;
    if nd == 7 {
        return Err("reserved ND value");
    }
    let _ = ecap & ECAP_PT;

    let root = frame::alloc_zeroed_frame().ok_or("no root-table frame")?;
    write64(base, REG_RTADDR, root.0);
    let mut command = GCMD_SRTP;
    write32(base, REG_GCMD, command);
    if !wait_status(base, GSTS_RTPS) {
        return Err("root-table pointer did not latch");
    }
    invalidate_context_global_raw(base)?;
    command |= GCMD_TE;
    write32(base, REG_GCMD, command);
    if !wait_status(base, GSTS_TES) {
        return Err("translation did not enable");
    }

    Ok(Unit {
        base,
        phys: drhd.base,
        cap,
        ecap,
        iotlb,
        haw: table.host_address_width,
        flags: table.flags,
        rmrr_count: table.rmrr.len(),
        atsr_count: table.atsr.len(),
        scope_count: drhd.scopes.len(),
        root: root.0,
        levels,
        next_domain: AtomicU16::new(1),
        context_tables: Mutex::new(BTreeMap::new()),
        domains: Mutex::new(BTreeMap::new()),
        by_sid: Mutex::new(BTreeMap::new()),
        table_frames: Mutex::new(alloc::vec![root.0]),
        started: AtomicBool::new(false),
    })
}

/// Start the fault-polling kernel thread once.
pub fn start_fault_worker() {
    let Some(unit) = unit() else {
        return;
    };
    if unit.started.swap(true, Ordering::AcqRel) {
        return;
    }
    match crate::sched::spawn_kernel("vtd-fault", fault_worker) {
        Ok(_) => {}
        Err(error) => {
            unit.started.store(false, Ordering::Release);
            kprintln!("vtd: fault worker not started: {error:?}");
        }
    }
}

fn fault_worker() {
    loop {
        poll_faults();
        let deadline = crate::time::now_ns().saturating_add(50_000_000);
        let _ = crate::sched::sleep_until(deadline);
    }
}

/// Drain all pending fault-recording registers and log each record.
pub fn poll_faults() {
    let Some(unit) = unit() else {
        return;
    };
    let fsts = read32(unit.base, REG_FSTS);
    if fsts == 0 {
        return;
    }
    if fsts & (FSTS_PPF | FSTS_PFO) != 0 {
        let nfr = usize::from(((unit.cap >> CAP_NFR_SHIFT) & 0xFF) as u8).saturating_add(1);
        let fro = ((unit.cap >> CAP_FRO_SHIFT) & 0x1FF) as usize;
        let mut index = 0usize;
        while index < nfr {
            let offset = fro.saturating_add(index).saturating_mul(16);
            let low = read64(unit.base, offset);
            let high = read64(unit.base, offset.saturating_add(8));
            let fault = decode_fault(low, high);
            if fault.present {
                kprintln!(
                    "vtd: fault sid={:#06x} reason={:#04x} ({}) addr={:#x} write={}",
                    fault.source_id,
                    fault.reason,
                    fault_reason_text(fault.reason),
                    fault.address,
                    fault.write
                );
                write32(unit.base, offset.saturating_add(12), 1 << 31);
            }
            index = index.saturating_add(1);
        }
    }
    write32(unit.base, REG_FSTS, fsts & FSTS_CLEAR);
}

/// Get or create the domain bound to `bdf`.
pub fn domain_for_bdf(bdf: Bdf) -> Result<Arc<Domain>, &'static str> {
    let unit = unit().ok_or("VT-d is not enabled")?;
    let sid = bdf.source_id();
    if let Some(id) = unit.by_sid.lock().get(&sid).copied()
        && let Some(domain) = unit.domains.lock().get(&id).cloned()
    {
        return Ok(domain);
    }

    let id = allocate_domain_id(&unit)?;
    let root = frame::alloc_zeroed_frame().ok_or("no domain root frame")?;
    let iova_end = 1u64
        .checked_shl(u32::from(unit.levels).saturating_mul(9).saturating_add(12))
        .ok_or("invalid IOVA width")?;
    let allocator = IovaAllocator::new(0x10_0000, iova_end).map_err(|_| "bad IOVA range")?;
    let domain = Arc::new(Domain {
        id,
        bdf,
        levels: unit.levels,
        root: root.0,
        iova: Mutex::new(allocator),
        tables: Mutex::new(alloc::vec![root.0]),
    });
    unit.install_context(&domain)?;
    unit.domains.lock().insert(id, Arc::clone(&domain));
    unit.by_sid.lock().insert(sid, id);
    kprintln!(
        "vtd: domain id={} bdf={} iova={:#x}-{:#x}",
        id,
        bdf.text(),
        domain.iova_start(),
        domain.iova_end()
    );
    Ok(domain)
}

/// Map a buffer's frames into a domain and return the bus IOVA.
pub fn map_buffer(
    domain: &Domain,
    frames: &[u64],
    len: u64,
    align: u64,
    limit: u64,
) -> Result<u64, &'static str> {
    let unit = unit().ok_or("VT-d is not enabled")?;
    domain.map(&unit, frames, len, align, limit)
}

/// Remove a buffer's IOVA mapping.
pub fn unmap_buffer(domain: &Domain, iova: u64, len: u64) {
    if let Some(unit) = unit() {
        domain.unmap(&unit, iova, len);
    }
}

/// Find the PCI function whose BAR or configuration page contains `phys`.
///
/// The S21 ABI has no explicit device argument for `DMA_ALLOC`; the caller's
/// most recent MMIO region is the device binding point. ECAM pages are decoded
/// directly; BAR ranges are matched by walking the first eight MCFG buses.
pub fn bdf_for_range(phys: u64, len: u64) -> Option<Bdf> {
    let info = acpi::info();
    if !info.mcfg_available {
        return None;
    }
    let start = info.mcfg_start_bus;
    let end = info.mcfg_end_bus;
    if end < start {
        return None;
    }
    let ecam_base = info.mcfg_base;
    let buses = u64::from(end.saturating_sub(start)).saturating_add(1);
    if let Some(ecam_len) = buses.checked_mul(1 << 20)
        && phys >= ecam_base
        && phys < ecam_base.checked_add(ecam_len)?
    {
        let offset = phys.saturating_sub(ecam_base);
        let bus = start.saturating_add(u8::try_from(offset >> 20).ok()?);
        let device = u8::try_from((offset >> 15) & 0x1F).ok()?;
        let function = u8::try_from((offset >> 12) & 0x7).ok()?;
        let _ = len;
        return Some(Bdf {
            bus,
            device,
            function,
        });
    }

    let scan_end = end.min(start.saturating_add(7));
    let mut bus = start;
    while bus <= scan_end {
        let bus_base = ecam_base.checked_add(u64::from(bus.saturating_sub(start)) << 20)?;
        let Ok(pointer) = mmio::map_mmio(bus_base, 1 << 20) else {
            bus = bus.saturating_add(1);
            continue;
        };
        let mut device = 0u8;
        while device < 32 {
            let mut function = 0u8;
            while function < 8 {
                let offset = usize::from(device)
                    .checked_mul(0x8000)?
                    .checked_add(usize::from(function).checked_mul(0x1000)?)?;
                // SAFETY: `offset` is inside the 1 MiB ECAM window mapped above.
                let config = unsafe { pointer.add(offset) };
                // SAFETY: config space is mapped and readable; a volatile word read has no
                // side effects beyond the device's configuration space semantics.
                let vendor = unsafe { core::ptr::read_volatile(config.cast::<u16>()) };
                if vendor != 0xFFFF {
                    let mut bar = 0usize;
                    while bar < 6 {
                        let bar_offset = 0x10usize.checked_add(bar.checked_mul(4)?)?;
                        // SAFETY: `config + bar_offset` is inside the same 4 KiB config page.
                        let value = unsafe {
                            core::ptr::read_volatile(config.add(bar_offset).cast::<u32>())
                        };
                        let mut base = u64::from(value & !0xF);
                        if value & 1 == 0 && (value >> 1) & 0x3 == 2 && bar.saturating_add(1) < 6 {
                            // SAFETY: the upper dword is inside the same config page.
                            let high = unsafe {
                                core::ptr::read_volatile(
                                    config.add(bar_offset.saturating_add(4)).cast::<u32>(),
                                )
                            };
                            base |= u64::from(high) << 32;
                        }
                        if value & 1 == 0 && base != 0 && (base & !0xF) == (phys & !0xF) {
                            return Some(Bdf {
                                bus,
                                device,
                                function,
                            });
                        }
                        bar = bar.saturating_add(1);
                    }
                }
                function = function.saturating_add(1);
            }
            device = device.saturating_add(1);
        }
        bus = bus.saturating_add(1);
    }
    None
}

fn allocate_domain_id(unit: &Unit) -> Result<u16, &'static str> {
    let mut attempts = 0u32;
    while attempts < u32::from(u16::MAX) {
        let id = unit.next_domain.fetch_add(1, Ordering::AcqRel);
        if id != 0 && !unit.domains.lock().contains_key(&id) {
            return Ok(id);
        }
        attempts = attempts.saturating_add(1);
    }
    Err("domain id space exhausted")
}

impl Unit {
    fn install_context(&self, domain: &Domain) -> Result<(), &'static str> {
        let bus = domain.bdf.bus;
        let mut context_tables = self.context_tables.lock();
        let context = if let Some(physical) = context_tables.get(&bus).copied() {
            physical
        } else {
            let context = frame::alloc_zeroed_frame().ok_or("no context-table frame")?;
            let root_slot = table_slot_stride(self.root, usize::from(bus), 16)?;
            // SAFETY: live root table; the bus index is bounded by 256.
            unsafe {
                core::ptr::write_volatile(root_slot, root_entry(context.0));
            }
            context_tables.insert(bus, context.0);
            self.table_frames.lock().push(context.0);
            context.0
        };
        drop(context_tables);
        let index =
            (usize::from(domain.bdf.device & 0x1F) << 3) | usize::from(domain.bdf.function & 0x7);
        let slot = table_slot_stride(context, index, 16)?;
        let (low, high) = context_entry(domain.id, domain.levels, domain.root);
        // SAFETY: live context table and a bounded device/function index. Write
        // the upper word first so a concurrent hardware walk never sees the
        // present bit with an uninitialised domain/AGAW.
        unsafe {
            core::ptr::write_volatile(slot.add(1), high);
            core::ptr::write_volatile(slot, low);
        }
        self.invalidate_context_global()
    }

    fn invalidate_context_global(&self) -> Result<(), &'static str> {
        invalidate_context_global_raw(self.base)
    }

    fn invalidate_iotlb_global(&self) -> Result<(), &'static str> {
        write64(
            self.iotlb,
            IOTLB_REG_OFF,
            IOTLB_IVT | IOTLB_IIRG_GLOBAL | IOTLB_DRAIN_READ | IOTLB_DRAIN_WRITE,
        );
        let mut attempts = 0usize;
        while attempts < WAIT_LOOPS {
            let value = read64(self.iotlb, IOTLB_REG_OFF);
            if value & IOTLB_IVT == 0 {
                if value & IOTLB_IAIG_GLOBAL == IOTLB_IAIG_GLOBAL {
                    return Ok(());
                }
                return Err("IOTLB invalidation failed");
            }
            core::hint::spin_loop();
            attempts = attempts.saturating_add(1);
        }
        Err("IOTLB invalidation timed out")
    }
}

fn invalidate_context_global_raw(base: usize) -> Result<(), &'static str> {
    write64(base, REG_CCMD, CCMD_ICC | CCMD_CIRG_GLOBAL);
    let mut attempts = 0usize;
    while attempts < WAIT_LOOPS {
        let value = read64(base, REG_CCMD);
        if value & CCMD_ICC == 0 {
            if value & CCMD_CAIG_GLOBAL == CCMD_CAIG_GLOBAL {
                return Ok(());
            }
            return Err("context-cache invalidation failed");
        }
        core::hint::spin_loop();
        attempts = attempts.saturating_add(1);
    }
    Err("context-cache invalidation timed out")
}

fn wait_status(base: usize, bit: u32) -> bool {
    let mut attempts = 0usize;
    while attempts < WAIT_LOOPS {
        if read32(base, REG_GSTS) & bit == bit {
            return true;
        }
        core::hint::spin_loop();
        attempts = attempts.saturating_add(1);
    }
    false
}

fn table_slot(table: u64, index: usize) -> Result<*mut u64, &'static str> {
    table_slot_stride(table, index, 8)
}

fn table_slot_stride(table: u64, index: usize, stride: usize) -> Result<*mut u64, &'static str> {
    if index >= TABLE_ENTRIES {
        return Err("page-table index out of range");
    }
    let address = table
        .checked_add(
            u64::try_from(index)
                .map_err(|_| "page-table index overflow")?
                .saturating_mul(stride as u64),
        )
        .ok_or("page-table address overflow")?;
    let pointer = frame::phys_to_virt(frame::PhysAddr(address));
    if pointer.is_null() {
        return Err("page-table frame is not mapped");
    }
    Ok(pointer.cast::<u64>())
}

/// Find the DMAR table in the captured ACPI bundle.
fn find_dmar() -> Option<&'static [u8]> {
    let blob = acpi::early_blob()?;
    if blob.len() < 24 || read_u32(blob, 0)? != acpi::ACPI_BLOB_MAGIC {
        return None;
    }
    let count = read_u32(blob, 16)? as usize;
    let directory = read_u32(blob, 20)? as usize;
    let mut index = 0usize;
    while index < count {
        let entry = directory.checked_add(index.checked_mul(20)?)?;
        let signature = blob.get(entry..entry.checked_add(4)?)?;
        if signature == b"DMAR" {
            let offset = read_u32(blob, entry.checked_add(8)?)? as usize;
            let length = read_u32(blob, entry.checked_add(12)?)? as usize;
            return blob.get(offset..offset.checked_add(length)?);
        }
        index = index.saturating_add(1);
    }
    None
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

fn read32(base: usize, offset: usize) -> u32 {
    let address = base.saturating_add(offset);
    // SAFETY: `address` is inside the mapped VT-d register page; reads have no side effects
    // beyond the documented status/command semantics.
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write32(base: usize, offset: usize, value: u32) {
    let address = base.saturating_add(offset);
    // SAFETY: same mapped register page; the caller writes only defined VT-d registers.
    unsafe {
        core::ptr::write_volatile(address as *mut u32, value);
    }
}

fn read64(base: usize, offset: usize) -> u64 {
    let address = base.saturating_add(offset);
    // SAFETY: same mapped register page, naturally aligned 64-bit register.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write64(base: usize, offset: usize, value: u64) {
    let address = base.saturating_add(offset);
    // SAFETY: same mapped register page, naturally aligned 64-bit register.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}
