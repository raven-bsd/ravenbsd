// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt objects: legacy IOAPIC lines and MSI/MSI-X vectors.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::arch::x86_64::msi::{DELIVERY_FIXED, msi_address, msi_data};
use crate::arch::x86_64::{ap_start, apic, ioapic, vector};
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

/// Live interrupt objects (S69 cleanup evidence).
static LIVE_IRQS: AtomicU64 = AtomicU64::new(0);
/// Temporary K9 KVM diagnostic: last 1 ms pump wait return.
static PUMP_LAST_WAKE: AtomicU64 = AtomicU64::new(0);

/// True when the `irq-debug` boot flag is set (K9 KVM pump-gap evidence only).
fn pump_trace() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "irq-debug")
}

/// Number of live interrupt objects across all processes.
pub fn live_objects() -> u64 {
    LIVE_IRQS.load(Ordering::Acquire)
}

/// True when the `irq-dist-trace` boot flag is set (S75a evidence only).
fn dist_trace() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "irq-dist-trace")
}

/// Choose the destination CPU for a new external IRQ.
///
/// With `smp_user=off` every external IRQ stays on the BSP (CPU 0). With `smp_user=on` the
/// destination round-robins over the application processors, leaving the BSP to the kernel timer
/// and the serial console.
fn select_irq_cpu() -> u32 {
    if !crate::sched::user_smp_enabled() {
        return 0;
    }
    let online = crate::sched::smp::online_cpus();
    if online <= 1 {
        return 0;
    }
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let ap_count = u32::try_from(online.saturating_sub(1)).unwrap_or(1);
    let slot = NEXT
        .fetch_add(1, Ordering::Relaxed)
        .checked_rem(ap_count)
        .unwrap_or(0);
    slot.saturating_add(1)
}

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

/// Claim registry for legacy GSIs.
///
/// A GSI has exactly one redirection entry: two `Irq` objects for the same line would overwrite
/// each other and the first close would mask the second object's line.  The bitmap is atomic
/// because creation and release run in process context.
mod gsi_owners {
    use core::sync::atomic::{AtomicU64, Ordering};

    use raven_abi::errno::Errno;

    /// Highest GSI tracked; the IOAPIC tables on supported machines stay far below it.
    const MAX_GSI: usize = 2048;
    const WORDS: usize = MAX_GSI / 64;

    static CLAIMED: [AtomicU64; WORDS] = [const { AtomicU64::new(0) }; WORDS];

    /// Claim `gsi` for one legacy `Irq`.
    pub fn claim(gsi: u32) -> Result<(), Errno> {
        let index = usize::try_from(gsi).map_err(|_| Errno::EINVAL)?;
        if index >= MAX_GSI {
            return Err(Errno::EINVAL);
        }
        let Some(slot) = CLAIMED.get(index / 64) else {
            return Err(Errno::EINVAL);
        };
        let mask = 1u64 << ((index % 64) as u32);
        if slot.fetch_or(mask, Ordering::AcqRel) & mask != 0 {
            Err(Errno::EBUSY)
        } else {
            Ok(())
        }
    }

    /// Release a claim after a close or a failed creation.
    pub fn release(gsi: u32) {
        let Ok(index) = usize::try_from(gsi) else {
            return;
        };
        if index >= MAX_GSI {
            return;
        }
        let Some(slot) = CLAIMED.get(index / 64) else {
            return;
        };
        slot.fetch_and(!(1u64 << ((index % 64) as u32)), Ordering::AcqRel);
    }
}

/// Interrupt-context spurious accounting.
///
/// The interrupt handler must not call the serial logger: the log lock is a plain spinlock that
/// a kernel thread can hold with interrupts enabled, so logging from the handler could
/// self-deadlock.  The handler only sets an atomic bit; the next IRQ syscall, in process context,
/// reports the accumulated vectors.
mod spurious {
    use core::sync::atomic::{AtomicU64, Ordering};

    const WORDS: usize = 4;

    static UNCLAIMED: [AtomicU64; WORDS] = [const { AtomicU64::new(0) }; WORDS];
    static UNEXPECTED: [AtomicU64; WORDS] = [const { AtomicU64::new(0) }; WORDS];

    fn note(bits: &[AtomicU64; WORDS], vector: u8) {
        let index = usize::from(vector);
        let Some(slot) = bits.get(index / 64) else {
            return;
        };
        slot.fetch_or(1u64 << ((index % 64) as u32), Ordering::Release);
    }

    /// A driver-pool vector fired with no owner: the message was acknowledged.
    pub fn unclaimed(vector: u8) {
        note(&UNCLAIMED, vector);
    }

    /// A vector outside the driver pool fired: nothing acknowledged it.
    pub fn unexpected(vector: u8) {
        note(&UNEXPECTED, vector);
    }

    /// Report and clear the accumulated vectors.  Process context only.
    pub fn flush() {
        report(&UNCLAIMED, "unclaimed vector", "(eoi)");
        report(&UNEXPECTED, "unexpected interrupt", "(no eoi)");
    }

    fn report(bits: &[AtomicU64; WORDS], what: &str, disposition: &str) {
        for (word, slot) in bits.iter().enumerate() {
            let mut pending = slot.swap(0, Ordering::AcqRel);
            while pending != 0 {
                let bit = pending.trailing_zeros();
                pending &= !(1u64 << bit);
                let vector = word
                    .saturating_mul(64)
                    .saturating_add(usize::try_from(bit).unwrap_or(usize::MAX));
                if let Ok(vector) = u8::try_from(vector) {
                    crate::kprintln!("irq: {} {:#04x} {}", what, vector, disposition);
                }
            }
        }
    }
}

/// Record an unclaimed driver-pool vector from interrupt context.
pub fn note_unclaimed(vector: u8) {
    spurious::unclaimed(vector);
}

/// Record an unexpected non-pool vector from interrupt context.
pub fn note_unexpected(vector: u8) {
    spurious::unexpected(vector);
}

/// Report the vectors recorded in interrupt context.  Called from IRQ syscalls only.
pub fn flush_spurious() {
    spurious::flush();
}

/// True when the boot command line asked for the legacy redirection-state probe.
fn legacy_probe_enabled() -> bool {
    crate::cmdline::CmdLine::parse(crate::memory::paging::cmdline()).flag("s30-irq-probe")
}

/// Print the real redirection-entry mask bit at legacy creation time.
fn probe_legacy_created(gsi: u32, vector: u8) {
    if !legacy_probe_enabled() {
        return;
    }
    match ioapic::redirection_masked(gsi) {
        Some(masked) => crate::kprintln!(
            "s30: legacy gsi={} vector={:#04x} masked={}",
            gsi,
            vector,
            u8::from(masked)
        ),
        None => {
            crate::kprintln!(
                "s30: legacy gsi={} vector={:#04x} masked=unknown",
                gsi,
                vector
            )
        }
    }
}

/// Print the real redirection-entry mask bit after the wait/ack unmask.
fn probe_legacy_unmasked(gsi: u32) {
    if !legacy_probe_enabled() {
        return;
    }
    match ioapic::redirection_masked(gsi) {
        Some(masked) => crate::kprintln!("s30: legacy gsi={} masked={}", gsi, u8::from(masked)),
        None => crate::kprintln!("s30: legacy gsi={} masked=unknown", gsi),
    }
}

/// One interrupt source a driver can wait on.
pub struct Irq {
    kind: IrqKind,
    gsi: u32,
    vector: u8,
    level: bool,
    armed: AtomicBool,
    /// The driver has armed the source (first wait or ack) or a firing was observed.
    live: AtomicBool,
    /// The driver confirmed, with `IRQ_ACK`, that it handled the last firing and quiesced the
    /// source.  Every new firing clears it again.
    quiesced: AtomicBool,
    /// Packed handle value of a handle that is being closed, with a valid marker; zero means
    /// no close is in progress.
    closing_handle: AtomicU64,
    pending: AtomicU64,
    waiters: WaitQueue,
    /// Resource release already happened (explicit process-exit retire or `Drop`).
    retired: AtomicBool,
    /// CPU that handled the most recent firing (S75a evidence).
    last_cpu: AtomicU32,
    /// One-shot "handler CPU" evidence print.
    announced: AtomicBool,
}

impl Irq {
    /// Stable watcher key shared with the IRQ_CREATE registration.
    pub(crate) fn event_key(&self) -> u64 {
        self as *const Irq as usize as u64
    }

    /// Firings accumulated since the last wait/ack.
    pub fn pending(&self) -> u64 {
        self.pending.load(Ordering::Acquire)
    }

    /// Consume and return the accumulated firings.
    fn take_pending(&self) -> Option<usize> {
        let count = self.pending.swap(0, Ordering::AcqRel);
        usize::try_from(count).ok().filter(|count| *count > 0)
    }

    /// True when `handle` is being closed by another thread.
    ///
    /// The flag is published with `Release` before [`Self::cancel_waiters`] takes the wait-queue
    /// lock, so a waiter that acquires the same lock afterwards is guaranteed to observe it
    /// (the lock's release-acquire pair orders the store before the waiter's condition check).
    fn cancelled(&self, handle: Handle) -> bool {
        self.closing_handle.load(Ordering::Acquire) == closing_key(handle.0)
    }

    /// Wake every waiter after a handle close.
    ///
    /// The waiter revalidates its own handle and returns `EIO` if it is gone; waiters on other
    /// handles of the same object simply re-block.  Called from process context, so the
    /// allocation in `wake_all` is allowed.
    ///
    /// Ordering: `closing_handle` is stored before the wait-queue lock is taken, and the
    /// waiter's enqueue condition re-checks the flag under that same lock.  A close therefore
    /// cannot slip between a waiter's cancel check and its enqueue: either the waiter is in
    /// the queue when the drain runs (it is woken), or it sees the flag and never enqueues.
    pub(crate) fn cancel_waiters(&self, handle: u32) {
        self.closing_handle
            .store(closing_key(handle), Ordering::Release);
        let _ = self.waiters.wake_all();
    }

    /// Wake and remove every waiter owned by `process_id` after that process exits.
    ///
    /// The dying thread may be the only holder of this object's `Arc`; without this sweep the
    /// waiter would stay queued, the object would never drop and the vector/GSI claim would
    /// leak.  Other processes waiting on the same shared interrupt stay queued.
    pub(crate) fn wake_process(&self, process_id: u64) {
        let _ = self.waiters.wake_process(process_id);
    }
}

/// Packed handle value with a "valid" marker so zero means "no handle is closing".
fn closing_key(handle: u32) -> u64 {
    u64::from(handle) | (1 << 32)
}

impl Irq {
    /// Release the GSI/vector claim once.
    fn release_resources(&self) {
        match self.kind {
            IrqKind::Legacy => {
                let _ = ioapic::mask(self.gsi);
                // Mask first, then let the next `irq_create` claim the line: a new owner must
                // never inherit an unmasked entry.
                gsi_owners::release(self.gsi);
                vector::release(self.vector);
            }
            IrqKind::Msi | IrqKind::MsiX => {
                if self.live.load(Ordering::Acquire) && !self.quiesced.load(Ordering::Acquire) {
                    // The driver armed the source but never confirmed it disabled the device.
                    // Retiring the vector is the fail-safe choice: an old message must not reach
                    // the next owner.
                    vector::quarantine(self.vector);
                } else {
                    vector::release(self.vector);
                }
            }
        }
    }

    /// Release the hardware claim when the last live process reference is gone.
    ///
    /// A thread blocked in `IRQ_WAIT` holds an `Arc<Irq>` on its kernel stack.  The scheduler
    /// abandons that stack on thread exit, so the `Arc` is never dropped and `Drop` alone would
    /// keep the GSI/vector claimed forever.  Process teardown calls this after checking that no
    /// other live process holds the same object; a second call is a no-op.
    pub(crate) fn retire(&self) {
        if !self.retired.swap(true, Ordering::AcqRel) {
            self.release_resources();
            LIVE_IRQS.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl Drop for Irq {
    fn drop(&mut self) {
        self.retire();
    }
}

/// `irq_create(root_dev, kind, number, flags) -> Handle`.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    super::require_root(&caller, Handle(frame.rdi as u32))?;
    let kind = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let number = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    let cpu = select_irq_cpu();

    let (irq, fill) = match kind {
        IRQ_LEGACY => {
            // The ABI's fourth argument travels in r10; r8 and r9 are ignored.
            if frame.r10 != 0 {
                return Err(Errno::EINVAL);
            }
            let irq_number = u8::try_from(number).map_err(|_| Errno::EINVAL)?;
            let gsi = ioapic::gsi_for_isa(irq_number);
            gsi_owners::claim(gsi)?;
            let (active_low, level) = crate::arch::x86_64::acpi::info()
                .override_for_isa(irq_number)
                .unwrap_or((false, false));
            let vector = match vector::allocate() {
                Some(vector) => vector,
                None => {
                    gsi_owners::release(gsi);
                    return Err(Errno::ENOSPC);
                }
            };
            let irq = Arc::new(Irq {
                kind: IrqKind::Legacy,
                gsi,
                vector,
                level,
                armed: AtomicBool::new(false),
                live: AtomicBool::new(false),
                quiesced: AtomicBool::new(false),
                closing_handle: AtomicU64::new(0),
                pending: AtomicU64::new(0),
                waiters: WaitQueue::new(),
                retired: AtomicBool::new(false),
                last_cpu: AtomicU32::new(0),
                announced: AtomicBool::new(false),
            });
            LIVE_IRQS.fetch_add(1, Ordering::AcqRel);
            vector::install_owner(vector, Arc::as_ptr(&irq));
            // `route` programs the entry masked; only IRQ_WAIT/IRQ_ACK unmask it.
            ioapic::route(
                gsi,
                vector,
                ap_start::apic_id(cpu as usize),
                active_low,
                level,
            )
            .map_err(|_| Errno::EINVAL)?;
            probe_legacy_created(gsi, vector);
            (irq, None)
        }
        IRQ_MSI | IRQ_MSIX => {
            let info = frame.r10;
            let (address_lo, address_hi) =
                msi_address(ap_start::apic_id(cpu as usize)).ok_or(Errno::EOPNOTSUPP)?;
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
                live: AtomicBool::new(false),
                quiesced: AtomicBool::new(false),
                closing_handle: AtomicU64::new(0),
                pending: AtomicU64::new(0),
                waiters: WaitQueue::new(),
                retired: AtomicBool::new(false),
                last_cpu: AtomicU32::new(0),
                announced: AtomicBool::new(false),
            });
            LIVE_IRQS.fetch_add(1, Ordering::AcqRel);
            vector::install_owner(vector, Arc::as_ptr(&irq));
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

    if dist_trace() {
        crate::kprintln!("irq-dist: vector {} -> cpu {}", irq.vector, cpu);
    }

    let rights = Rights::READ.union(Rights::DUP).union(Rights::TRANSFER);
    Ok(caller.handles.lock().insert(Object::Irq(irq), rights)?.0 as usize)
}

/// `irq_wait(irq, timeout_ns) -> count`.
pub fn wait(frame: &SyscallFrame) -> Result<usize, Errno> {
    flush_spurious();
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
    if irq.kind != IrqKind::Legacy {
        // Waiting declares the source live: the driver has armed it.  Closing without the final
        // IRQ_ACK quarantines the vector.
        irq.live.store(true, Ordering::Release);
    }
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let start = time::now_ns();
    let deadline = (timeout_ns != 0).then(|| start.saturating_add(timeout_ns));
    let trace = timeout_ns == 1_000_000 && pump_trace();
    if trace {
        let previous = PUMP_LAST_WAKE.load(Ordering::Acquire);
        if previous != 0 && start.saturating_sub(previous) > 2_000_000 {
            crate::kprintln!(
                "IRQP gap gap_us={} now={}",
                start.saturating_sub(previous) / 1_000,
                start
            );
        }
    }
    // A legacy line stays masked until its first wait (or ack), then is re-armed by each ack.
    if irq.kind == IrqKind::Legacy && !irq.armed.swap(true, Ordering::AcqRel) {
        ioapic::unmask(irq.gsi).map_err(|_| Errno::EINVAL)?;
        probe_legacy_unmasked(irq.gsi);
    }

    loop {
        // A concurrent HANDLE_CLOSE wakes the wait queue.  The close syscall runs to completion
        // on this non-preemptible CPU before this thread resumes, so the handle is gone here.
        if irq.cancelled(handle) || caller.handles.lock().get(handle, Rights::READ).is_err() {
            return Err(Errno::EIO);
        }
        if let Some(count) = irq.take_pending() {
            if trace {
                let now = time::now_ns();
                let previous = PUMP_LAST_WAKE.swap(now, Ordering::AcqRel);
                if previous != 0 && now.saturating_sub(previous) > 2_000_000 {
                    crate::kprintln!(
                        "IRQP irq gap_us={} wait_us={} now={}",
                        now.saturating_sub(previous) / 1_000,
                        now.saturating_sub(start) / 1_000,
                        now
                    );
                }
            }
            if dist_trace() && !irq.announced.swap(true, Ordering::AcqRel) {
                let handled = irq.last_cpu.load(Ordering::Acquire);
                if handled != 0 {
                    crate::kprintln!("irq-dist: vector {} handled on cpu {}", irq.vector, handled);
                }
            }
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
        // The enqueue decision re-checks the cancel flag under the wait-queue lock.
        // `cancel_waiters` stores `closing_handle` and then takes the same lock to drain, so
        // the lock ordering alone proves the pair cannot miss each other: either this push
        // happens before the drain (the close wakes the waiter), or the condition observes
        // the close and refuses to enqueue (the check above returns `EIO` on the next pass).
        // Without this, the window between the cancel check and the block would be safe only
        // because syscall entry masks interrupts on the delivery CPU.
        let queued = irq.waiters.enqueue_if(0, u64::from(irq.vector), || {
            Ok(irq.pending() == 0 && !irq.cancelled(handle))
        });
        match queued {
            Ok(true) => {
                let status = sched::block_current();
                irq.waiters.remove(0, u64::from(irq.vector), thread_id);
                sched::end_block(token);
                if status == Errno::ETIMEDOUT.to_ret() {
                    if trace {
                        let now = time::now_ns();
                        let previous = PUMP_LAST_WAKE.swap(now, Ordering::AcqRel);
                        let late = now.saturating_sub(start.saturating_add(timeout_ns));
                        if late > 1_000_000 {
                            crate::kprintln!(
                                "IRQP late wait_us={} late_us={} gap_us={} now={}",
                                now.saturating_sub(start) / 1_000,
                                late / 1_000,
                                if previous == 0 {
                                    0
                                } else {
                                    now.saturating_sub(previous) / 1_000
                                },
                                now
                            );
                        }
                    }
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
    flush_spurious();
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let irq = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::READ)? {
            Object::Irq(irq) => Arc::clone(irq),
            _ => return Err(Errno::EBADF),
        }
    };
    if irq.kind == IrqKind::Legacy {
        if irq.level || !irq.armed.swap(true, Ordering::AcqRel) {
            ioapic::unmask(irq.gsi).map_err(|_| Errno::EINVAL)?;
            probe_legacy_unmasked(irq.gsi);
        }
    } else {
        // The driver's quiesce barrier: it handled the last firing and disabled the source.
        irq.live.store(true, Ordering::Release);
        irq.quiesced.store(true, Ordering::Release);
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
    let Some(irq) = vector::handle_owner(vector) else {
        // An interrupt for a retired pool vector (closed source or quarantine): there is no
        // object to charge, but the message was really delivered, so acknowledge it.
        note_unclaimed(vector);
        apic::eoi();
        return true;
    };
    let irq_key = irq.event_key();
    let irq = &*irq;
    let _ = irq.pending.fetch_add(1, Ordering::AcqRel);
    irq.last_cpu.store(
        u32::try_from(ap_start::this_cpu()).unwrap_or(0),
        Ordering::Release,
    );
    if irq.kind != IrqKind::Legacy {
        // The source is live again, so the driver's last ack no longer quiesces it.
        irq.live.store(true, Ordering::Release);
        irq.quiesced.store(false, Ordering::Release);
    }
    if irq.level {
        // Storm prevention: a level line must not re-enter before the driver acked the device.
        ioapic::mask_gsi_isr(irq.gsi);
    }
    let _ = irq.waiters.wake_one(0, u64::from(irq.vector));
    event::notify_source(irq_key);
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
