// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt objects: legacy IOAPIC lines and MSI/MSI-X vectors.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::arch::x86_64::msi::{DELIVERY_FIXED, msi_address, msi_data};
use crate::arch::x86_64::{apic, ioapic, vector};
use crate::event;
use crate::process::{self, handle::Object};
use crate::sched;
use crate::sched::wait::WaitQueue;
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::copy_to_user;

/// `IRQ_CREATE` kinds.
pub const IRQ_LEGACY: u32 = 1;
/// `IRQ_CREATE` kinds.
pub const IRQ_MSI: u32 = 2;
/// `IRQ_CREATE` kinds.
pub const IRQ_MSIX: u32 = 3;

/// The user-visible `IrqInfo` record. Its layout is fixed by the ABI syscall documentation.
#[repr(C)]
struct IrqInfo {
    vector: u32,
    address_lo: u32,
    address_hi: u32,
    data: u32,
}

/// How the interrupt source is wired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IrqKind {
    /// An ISA IRQ routed through the IOAPIC.
    Legacy,
    /// An MSI message.
    Msi,
    /// An MSI-X table entry (same message encoding, programmed by the driver).
    MsiX,
}

/// One interrupt source a driver can wait on.
pub struct Irq {
    kind: IrqKind,
    gsi: u32,
    vector: u8,
    level: bool,
    armed: AtomicBool,
    pending: AtomicU64,
    waiters: WaitQueue,
}

impl Irq {
    /// Firings accumulated since the last wait/ack.
    pub fn pending(&self) -> u64 {
        self.pending.load(Ordering::Acquire)
    }

    /// Consume and return the accumulated firings.
    fn take_pending(&self) -> Option<usize> {
        let count = self.pending.swap(0, Ordering::AcqRel);
        usize::try_from(count).ok().filter(|count| *count > 0)
    }
}

impl Drop for Irq {
    fn drop(&mut self) {
        if self.kind == IrqKind::Legacy {
            let _ = ioapic::mask(self.gsi);
        }
        vector::release(self.vector);
    }
}

/// `irq_create(root_dev, kind, number, flags) -> Handle`.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    super::require_root(&caller, Handle(frame.rdi as u32))?;
    let kind = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let number = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;

    let (irq, fill) = match kind {
        IRQ_LEGACY => {
            if frame.r8 != 0 {
                return Err(Errno::EINVAL);
            }
            let irq_number = u8::try_from(number).map_err(|_| Errno::EINVAL)?;
            let gsi = ioapic::gsi_for_isa(irq_number);
            let (active_low, level) = crate::arch::x86_64::acpi::info()
                .override_for_isa(irq_number)
                .unwrap_or((false, false));
            let vector = vector::allocate().ok_or(Errno::ENOSPC)?;
            let irq = Arc::new(Irq {
                kind: IrqKind::Legacy,
                gsi,
                vector,
                level,
                armed: AtomicBool::new(false),
                pending: AtomicU64::new(0),
                waiters: WaitQueue::new(),
            });
            vector::install_owner(vector, Arc::as_ptr(&irq) as u64);
            ioapic::route(gsi, vector, apic::id(), active_low, level).map_err(|_| Errno::EINVAL)?;
            // The line stays masked until the first IRQ_WAIT / IRQ_ACK.
            ioapic::mask(gsi).map_err(|_| Errno::EINVAL)?;
            (irq, None)
        }
        IRQ_MSI | IRQ_MSIX => {
            let info = frame.r10;
            let vector = vector::allocate().ok_or(Errno::ENOSPC)?;
            let irq = Arc::new(Irq {
                kind: if kind == IRQ_MSI {
                    IrqKind::Msi
                } else {
                    IrqKind::MsiX
                },
                gsi: 0,
                vector,
                level: false,
                armed: AtomicBool::new(true),
                pending: AtomicU64::new(0),
                waiters: WaitQueue::new(),
            });
            vector::install_owner(vector, Arc::as_ptr(&irq) as u64);
            let (address_lo, address_hi) = msi_address(apic::id(), apic::is_x2apic());
            let data = msi_data(vector, DELIVERY_FIXED, false);
            let fill = IrqInfo {
                vector: u32::from(vector),
                address_lo,
                address_hi,
                data,
            };
            (irq, Some((info, fill)))
        }
        _ => return Err(Errno::EINVAL),
    };

    if let Some((pointer, fill)) = fill {
        let destination = UserSlice::new(pointer, 16)?;
        let bytes = fill.as_bytes();
        copy_to_user(destination, &bytes)?;
    }

    let rights = Rights::READ.union(Rights::DUP).union(Rights::TRANSFER);
    Ok(caller.handles.lock().insert(Object::Irq(irq), rights)?.0 as usize)
}

/// `irq_wait(irq, timeout_ns) -> count`.
pub fn wait(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let timeout_ns = frame.rsi;
    let irq = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(irq),
            _ => return Err(Errno::EBADF),
        }
    };
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let deadline = (timeout_ns != 0).then(|| time::now_ns().saturating_add(timeout_ns));
    // A legacy line stays masked until its first wait (or ack), then is re-armed by each ack.
    if irq.kind == IrqKind::Legacy && !irq.armed.swap(true, Ordering::AcqRel) {
        ioapic::unmask(irq.gsi).map_err(|_| Errno::EINVAL)?;
    }

    loop {
        if let Some(count) = irq.take_pending() {
            return Ok(count);
        }
        if let Some(deadline) = deadline
            && time::now_ns() >= deadline
        {
            return Ok(0);
        }
        let token = sched::begin_block()?;
        if let Some(deadline) = deadline {
            sched::add_sleep(token, deadline, Errno::ETIMEDOUT.to_ret());
        }
        let queued = irq
            .waiters
            .enqueue_if(0, u64::from(irq.vector), || Ok(irq.pending() == 0));
        match queued {
            Ok(true) => {
                let status = sched::block_current();
                irq.waiters.remove(0, u64::from(irq.vector), thread_id);
                sched::end_block(token);
                if status == Errno::ETIMEDOUT.to_ret() {
                    return Ok(0);
                }
            }
            Ok(false) => {
                sched::end_block(token);
            }
            Err(error) => {
                sched::end_block(token);
                return Err(error);
            }
        }
    }
}

/// `irq_ack(irq) -> 0`.
pub fn ack(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let irq = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(irq),
            _ => return Err(Errno::EBADF),
        }
    };
    if irq.kind == IrqKind::Legacy && (irq.level || !irq.armed.swap(true, Ordering::AcqRel)) {
        ioapic::unmask(irq.gsi).map_err(|_| Errno::EINVAL)?;
    }
    Ok(0)
}

/// Handle a device interrupt: increment the owner's counter, wake waiters, signal event queues
/// and send the end-of-interrupt.
///
/// Runs on the delivery CPU with interrupts masked by the CPU: no allocation, no blocking and no
/// process-context lock.  Returns false for vectors outside the driver pool.
pub fn handle_interrupt(vector: u8) -> bool {
    if !vector::is_driver_vector(vector) {
        return false;
    }
    let owner = vector::owner(vector);
    if owner == 0 {
        // An interrupt for a freed pool vector: acknowledge it and count it as handled.
        apic::eoi();
        return true;
    }
    // SAFETY: `install_owner` publishes the pointer before the source can fire, and `release`
    // clears the slot before the object is dropped; the delivery CPU is the boot CPU, where
    // process-context close operations cannot run concurrently with this handler.
    let irq = unsafe { &*(owner as *const Irq) };
    let _ = irq.pending.fetch_add(1, Ordering::AcqRel);
    if irq.level {
        // Storm prevention: a level line must not re-enter before the driver acked the device.
        ioapic::mask_gsi_isr(irq.gsi);
    }
    let _ = irq.waiters.wake_one(0, u64::from(irq.vector));
    event::notify_all();
    apic::eoi();
    true
}

impl IrqInfo {
    fn as_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        let mut at = 0usize;
        for value in [self.vector, self.address_lo, self.address_hi, self.data] {
            let end = at.checked_add(4).unwrap_or(0);
            if let Some(slot) = bytes.get_mut(at..end) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
            at = end;
        }
        bytes
    }
}
