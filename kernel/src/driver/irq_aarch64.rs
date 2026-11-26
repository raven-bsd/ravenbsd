// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt objects on GICv2 (QEMU `virt` and the Pi 4 GIC-400).
//!
//! An `Irq` owns one shared peripheral interrupt (SPI). The object keeps the line masked until
//! the first `IRQ_WAIT` (or `IRQ_ACK`), masks a level line again after every delivery, and
//! releases the SPI claim when the last handle closes. A GICv2m MSI frame, when the firmware
//! ACPI MADT advertises one, provides the MSI-X doorbell and SPI pool; without that frame
//! `IRQ_CREATE(MSIX)` reports `EOPNOTSUPP` instead of inventing a vector ABI.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::arch::aarch64::gic;
use crate::event;
use crate::process::{self, handle::Object};
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::copy_to_user;

/// `IRQ_CREATE` kinds.
pub const IRQ_LEGACY: u32 = 1;
/// `IRQ_CREATE` kinds.
pub const IRQ_MSI: u32 = 2;
/// `IRQ_CREATE` kinds.
pub const IRQ_MSIX: u32 = 3;

/// Live interrupt objects (S69 cleanup evidence).
static LIVE_IRQS: AtomicU64 = AtomicU64::new(0);

/// Number of live interrupt objects across all processes.
pub fn live_objects() -> u64 {
    LIVE_IRQS.load(Ordering::Acquire)
}

/// Highest SPI id the GICv2 distributor can carry.
const MAX_SPI: u32 = 1020;
/// Registry slots (one per possible interrupt id).
const SLOTS: usize = MAX_SPI as usize;
/// Registry value between claim and publication.
const CLAIMED: u64 = 1;
/// Priority used for every driver line.
const PRIORITY: u8 = 0xa0;

/// GICv2m MSI frame size; the Typer and Set SPI registers live in the first page.
const V2M_FRAME_SIZE: usize = 0x1000;
/// GICv2m `MSI_TYPER`: SPI base in bits 25:16, SPI count in bits 9:0.
const GIC_MSI_TYPER: usize = 0x0008;
/// GICv2m `MSI_SETSPI`: writing an SPI number raises that interrupt.
const GIC_MSI_SETSPI: u64 = 0x0040;
/// MADT Generic MSI Frame flags: `SpiCount`/`SpiBase` override the Typer register.
const MADT_OVERRIDE_SPI_VALUES: u32 = 1;
/// MADT table type of a Generic MSI Frame (ACPI 5.1, "GICv2m").
const MADT_GENERIC_MSI_FRAME: u8 = 0x0d;
/// MADT offset of the first interrupt-controller entry: the 36-byte SDT header plus the
/// Local Interrupt Controller Address and Flags fields.
const MADT_ENTRIES_OFFSET: usize = 44;

/// One GICv2m MSI frame found in the firmware MADT.
#[derive(Clone, Copy)]
struct V2mFrame {
    /// Physical base of the MSI frame (the Set SPI doorbell is at `base + 0x40`).
    base: u64,
    /// First SPI the frame can raise.
    spi_base: u16,
    /// Number of contiguous SPIs the frame owns.
    spi_count: u16,
}

/// The discovered frame, cached after the first `IRQ_CREATE(MSIX)`.
static V2M: spin::Mutex<Option<V2mFrame>> = spin::Mutex::new(None);

/// The user-visible `IrqInfo` record, fixed by the ABI syscall documentation.
#[repr(C)]
struct IrqInfo {
    vector: u32,
    address_lo: u32,
    address_hi: u32,
    data: u32,
}

impl IrqInfo {
    fn as_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        let mut at = 0usize;
        for value in [self.vector, self.address_lo, self.address_hi, self.data] {
            let end = at.saturating_add(4);
            if let Some(slot) = bytes.get_mut(at..end) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
            at = end;
        }
        bytes
    }
}

/// One claimed SPI's shared state.
///
/// The GIC handler reaches this through the registry's raw pointer; a waiter holds its own
/// `Arc` so closing the last handle can still run [`Irq::drop`] and wake the waiter.
struct SpiState {
    spi: u32,
    level: bool,
    armed: AtomicBool,
    pending: AtomicU64,
    closed: AtomicBool,
}

impl SpiState {
    fn take_pending(&self) -> Option<usize> {
        let count = self.pending.swap(0, Ordering::AcqRel);
        usize::try_from(count).ok().filter(|count| *count > 0)
    }
}

/// One interrupt source a driver can wait on.
pub struct Irq {
    state: Arc<SpiState>,
    /// Resource release already happened (explicit process-exit retire or `Drop`).
    retired: AtomicBool,
}

impl Irq {
    /// Stable watcher key shared with the IRQ_CREATE registration (the SPI state allocation).
    pub(crate) fn event_key(&self) -> u64 {
        Arc::as_ptr(&self.state) as usize as u64
    }

    /// Firings accumulated since the last wait/ack.
    pub fn pending(&self) -> u64 {
        self.state.pending.load(Ordering::Acquire)
    }

    /// Process-exit wait sweeper.
    ///
    /// aarch64 `IRQ_WAIT` polls with interrupts enabled instead of blocking on a wait queue, so
    /// there is no queue entry to remove; the socket still exists for the shared process-exit
    /// sweep.
    pub(crate) fn wake_process(&self, process_id: u64) {
        let _ = process_id;
    }

    /// Release the SPI claim once.
    ///
    /// aarch64 `IRQ_WAIT` polls and keeps only an `Arc<SpiState>`, but a process-exit sweep still
    /// retires the claim so a line is immediately reusable even if a stale object reference
    /// survives on an abandoned stack.
    pub(crate) fn retire(&self) {
        if !self.retired.swap(true, Ordering::AcqRel) {
            self.release_resources();
            LIVE_IRQS.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn release_resources(&self) {
        let spi = self.state.spi;
        self.state.closed.store(true, Ordering::Release);
        let _ = gic::disable(spi);
        let _ = gic::clear_pending(spi);
        let published = Arc::as_ptr(&self.state) as u64;
        {
            // The handler runs on the boot CPU with IRQs masked; masking them here makes the
            // registry clear and the object release one critical section from the handler's
            // view. A handler already in flight has completed before this process context runs.
            let _guard = crate::sync::critical();
            if let Some(slot) = slot(spi) {
                let _ = slot.compare_exchange(published, 0, Ordering::AcqRel, Ordering::Acquire);
            }
        }
    }
}

impl Drop for Irq {
    fn drop(&mut self) {
        self.retire();
    }
}

/// The raw owner registry. Zero is free, [`CLAIMED`] is a creation in progress, any other value
/// is the published `SpiState` pointer.
static OWNERS: [AtomicU64; SLOTS] = [const { AtomicU64::new(0) }; SLOTS];

fn slot(spi: u32) -> Option<&'static AtomicU64> {
    OWNERS.get(usize::try_from(spi).ok()?)
}

/// The GICv2m frame discovered from the firmware MADT, if the platform has one.
fn v2m_frame() -> Option<V2mFrame> {
    let mut cached = V2M.lock();
    if let Some(frame) = *cached {
        return Some(frame);
    }
    let discovered = discover_v2m()?;
    *cached = Some(discovered);
    Some(discovered)
}

/// Read the GICv2m base and SPI pool from the captured ACPI MADT.
///
/// The MADT names the frame's physical base. Firmware may also override the SPI base/count in
/// the entry flags; otherwise the frame's `MSI_TYPER` register is authoritative, as the NetBSD
/// `gic_v2m_acpi_find_msi_frame` donor does.
fn discover_v2m() -> Option<V2mFrame> {
    let Some(madt) = crate::arch::pcie::acpi_table(b"APIC") else {
        crate::kprintln!("irq: GICv2m unavailable: no MADT in the ACPI blob");
        return None;
    };
    let Some((frame_base, override_values)) = msi_frame_entry(madt) else {
        crate::kprintln!("irq: GICv2m unavailable: MADT has no generic MSI frame");
        return None;
    };
    let (spi_base, spi_count) = match override_values {
        Some(values) => values,
        None => {
            let Some(pointer) = crate::memory::mmio::map_mmio(frame_base, V2M_FRAME_SIZE).ok()
            else {
                crate::kprintln!("irq: GICv2m frame map failed at {frame_base:#x}");
                return None;
            };
            // SAFETY: `map_mmio` mapped the frame's first page as device memory and the mapping
            // is never unmapped; the Typer register is a 4-byte aligned read-only register.
            let typer =
                unsafe { core::ptr::read_volatile(pointer.add(GIC_MSI_TYPER).cast::<u32>()) };
            let base = u16::try_from((typer >> 16) & 0x3ff).ok()?;
            let count = u16::try_from(typer & 0x3ff).ok()?;
            (base, count)
        }
    };
    if spi_count == 0 {
        crate::kprintln!("irq: GICv2m frame at {frame_base:#x} owns no SPI");
        return None;
    }
    crate::kprintln!(
        "irq: GICv2m frame {frame_base:#x} SPIs {spi_base}..{}",
        u32::from(spi_base).saturating_add(u32::from(spi_count))
    );
    Some(V2mFrame {
        base: frame_base,
        spi_base,
        spi_count,
    })
}

/// Find the first Generic MSI Frame entry in a MADT and return its base and optional override.
fn msi_frame_entry(madt: &[u8]) -> Option<(u64, Option<(u16, u16)>)> {
    if madt.get(..4)? != b"APIC" {
        return None;
    }
    let length = usize::try_from(read_u32(madt, 4)?).ok()?;
    let end = length.min(madt.len());
    let mut at = MADT_ENTRIES_OFFSET;
    while at.checked_add(2)? <= end {
        let kind = *madt.get(at)?;
        let size = usize::from(*madt.get(at.checked_add(1)?)?);
        if size < 2 {
            return None;
        }
        let next = at.checked_add(size)?;
        if next > end {
            return None;
        }
        if kind == MADT_GENERIC_MSI_FRAME {
            let entry = madt.get(at..next)?;
            if entry.len() < 24 {
                return None;
            }
            let base = read_u64(entry, 8)?;
            let flags = read_u32(entry, 16)?;
            let override_values = if flags & MADT_OVERRIDE_SPI_VALUES != 0 {
                Some((read_u16(entry, 22)?, read_u16(entry, 20)?))
            } else {
                None
            };
            return Some((base, override_values));
        }
        at = next;
    }
    None
}

/// Claim the first free SPI in the frame's pool.
fn claim_v2m_spi(frame: V2mFrame) -> Result<u32, Errno> {
    let first = u32::from(frame.spi_base);
    let end = first
        .saturating_add(u32::from(frame.spi_count))
        .min(MAX_SPI);
    let mut spi = first;
    while spi < end {
        let Some(slot) = slot(spi) else {
            return Err(Errno::EINVAL);
        };
        if slot
            .compare_exchange(0, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return Ok(spi);
        }
        spi = spi.saturating_add(1);
    }
    Err(Errno::ENOSPC)
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

/// Handle a claimed SPI from the interrupt vector.
///
/// Returns false when the line is unclaimed; the vector then masks it and counts the event.
pub fn handle_spi(spi: u32) -> bool {
    let Some(slot) = slot(spi) else {
        return false;
    };
    let published = slot.load(Ordering::Acquire);
    if published == 0 || published == CLAIMED {
        return false;
    }
    // SAFETY: `published` was installed by `create` before the source could fire, and `Drop`
    // clears the slot with interrupts masked on the delivery CPU before releasing the object.
    let state = unsafe { &*(published as *const SpiState) };
    if state.level {
        // Storm prevention: a level line is masked before EOI and stays masked until IRQ_ACK.
        let _ = gic::disable(spi);
    }
    state.pending.fetch_add(1, Ordering::AcqRel);
    event::notify_source(published);
    true
}

/// Report and clear the interrupt-context counters. Process context only.
pub fn flush_spurious() {
    crate::arch::aarch64::irq::flush_spurious();
}

/// `irq_create(root_dev, kind, number, flags) -> Handle`.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    super::require_root(&caller, Handle(frame.rdi as u32))?;
    let kind = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let number = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;

    match kind {
        IRQ_LEGACY => {
            // The ABI's fourth argument travels in r10; r8 and r9 are ignored.
            if frame.r10 != 0 {
                return Err(Errno::EINVAL);
            }
            if !gic::is_spi(number) {
                return Err(Errno::EINVAL);
            }
            let route = crate::arch::aarch64::fdt::spi_route(number);
            if route.active_low {
                // GIC-400 SPIs are wired active-high; an active-low tree flag would need a
                // board-level inverter or a different controller, neither of which exists here.
                return Err(Errno::EINVAL);
            }
            let slot = slot(number).ok_or(Errno::EINVAL)?;
            if slot
                .compare_exchange(0, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err(Errno::EBUSY);
            }
            let configured = gic::set_priority(number, PRIORITY).is_ok()
                // CPU 0 is the boot CPU index; the GIC driver converts it to ITARGETSR mask 1.
                && gic::set_target(number, 0).is_ok()
                && gic::configure_spi(number, route.level).is_ok()
                && gic::disable(number).is_ok()
                && gic::clear_pending(number).is_ok();
            if !configured {
                slot.store(0, Ordering::Release);
                return Err(Errno::EINVAL);
            }
            let state = Arc::new(SpiState {
                spi: number,
                level: route.level,
                armed: AtomicBool::new(false),
                pending: AtomicU64::new(0),
                closed: AtomicBool::new(false),
            });
            slot.store(Arc::as_ptr(&state) as u64, Ordering::Release);
            let irq = Arc::new(Irq {
                state,
                retired: AtomicBool::new(false),
            });
            LIVE_IRQS.fetch_add(1, Ordering::AcqRel);
            let rights = Rights::READ.union(Rights::DUP).union(Rights::TRANSFER);
            Ok(caller.handles.lock().insert(Object::Irq(irq), rights)?.0 as usize)
        }
        IRQ_MSIX => {
            let v2m = v2m_frame().ok_or(Errno::EOPNOTSUPP)?;
            // The fourth argument is the writable `IrqInfo` the driver reads the message from.
            let destination = UserSlice::new(frame.r10, 16)?;
            let spi = claim_v2m_spi(v2m)?;
            // MSI is edge-triggered; the frame's SPI behaves like any other peripheral SPI.
            let configured = gic::set_priority(spi, PRIORITY).is_ok()
                && gic::set_target(spi, 0).is_ok()
                && gic::configure_spi(spi, false).is_ok()
                && gic::disable(spi).is_ok()
                && gic::clear_pending(spi).is_ok();
            if !configured {
                if let Some(slot) = slot(spi) {
                    slot.store(0, Ordering::Release);
                }
                return Err(Errno::EINVAL);
            }
            let doorbell = v2m.base.saturating_add(GIC_MSI_SETSPI);
            let fill = IrqInfo {
                vector: spi,
                address_lo: doorbell as u32,
                address_hi: (doorbell >> 32) as u32,
                data: spi,
            };
            if let Err(error) = copy_to_user(destination, &fill.as_bytes()) {
                if let Some(slot) = slot(spi) {
                    slot.store(0, Ordering::Release);
                }
                let _ = gic::disable(spi);
                let _ = gic::clear_pending(spi);
                return Err(error);
            }
            let state = Arc::new(SpiState {
                spi,
                level: false,
                armed: AtomicBool::new(false),
                pending: AtomicU64::new(0),
                closed: AtomicBool::new(false),
            });
            if let Some(slot) = slot(spi) {
                slot.store(Arc::as_ptr(&state) as u64, Ordering::Release);
            }
            let irq = Arc::new(Irq {
                state,
                retired: AtomicBool::new(false),
            });
            LIVE_IRQS.fetch_add(1, Ordering::AcqRel);
            let rights = Rights::READ.union(Rights::DUP).union(Rights::TRANSFER);
            Ok(caller.handles.lock().insert(Object::Irq(irq), rights)?.0 as usize)
        }
        // GICv2m also carries plain MSI, but no Raven driver requests it and the s21d aarch64
        // fixture pins the existing `IRQ_MSI` negative contract; MSI-X is the device path.
        IRQ_MSI => Err(Errno::EOPNOTSUPP),
        _ => Err(Errno::EINVAL),
    }
}

/// `irq_wait(irq, timeout_ns) -> count`.
pub fn wait(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let timeout_ns = frame.rsi;
    let state = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(&irq.state),
            _ => return Err(Errno::EBADF),
        }
    };
    // A line stays masked until its first wait (or ack), then each ack re-arms it.
    if !state.armed.swap(true, Ordering::AcqRel) {
        gic::enable(state.spi).map_err(|_| Errno::EINVAL)?;
    }
    let deadline = (timeout_ns != 0).then(|| time::now_ns().saturating_add(timeout_ns));
    // The aarch64 idle context is not re-entered after the first context switch, so a blocked
    // syscall would never be woken by the timer. Poll with interrupts enabled instead: the timer
    // keeps time advancing and the GIC handler publishes the firing. A scheduler fix can restore
    // blocking without changing the object or the ABI.
    // SAFETY: the vector table, GIC and timer are initialised before `/init` runs.
    unsafe {
        crate::sync::enable_interrupts();
    }
    loop {
        if state.closed.load(Ordering::Acquire)
            || caller.handles.lock().get(handle, Rights::READ).is_err()
        {
            return Err(Errno::EIO);
        }
        if let Some(count) = state.take_pending() {
            return Ok(count);
        }
        if let Some(deadline) = deadline
            && time::now_ns() >= deadline
        {
            return Ok(0);
        }
        core::hint::spin_loop();
    }
}

/// `irq_ack(irq) -> 0`.
pub fn ack(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let state = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(&irq.state),
            _ => return Err(Errno::EBADF),
        }
    };
    if state.closed.load(Ordering::Acquire) {
        return Err(Errno::EIO);
    }
    state.armed.store(true, Ordering::Release);
    // A level source may have set the GIC pending bit again while the line was masked; drop it
    // before re-arming so the next wait starts from a clean edge/level.
    let _ = gic::clear_pending(state.spi);
    gic::enable(state.spi).map_err(|_| Errno::EINVAL)?;
    Ok(0)
}
