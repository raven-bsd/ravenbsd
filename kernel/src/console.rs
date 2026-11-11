// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Console input: COM1 receive interrupt, ring buffer and reader wait queue.
//!
//! The 16550 raises the "received data available" interrupt on ISA IRQ 4.  The handler drains
//! the UART into a lock-free [`ringbuf::RxRing`] (4 KiB), sends the local-APIC EOI and wakes one
//! waiting reader.  When the IOAPIC is unavailable the UART interrupt is disarmed and the timer
//! tick calls [`poll`] instead; the fallback is logged at boot.
//!
//! The interrupt handler never allocates, never blocks and never takes a sleepable lock.  The
//! ring is lock-free (one producer, one consumer); the only lock on this path is the wait queue's
//! interrupt-safe queue lock, held while a waiter is selected and released before the scheduler
//! wake.  Readers hold no lock while blocked or while copying to user memory.

#[path = "../../crates/raven-ringbuf/src/lib.rs"]
mod ringbuf;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::arch::x86_64::{acpi, apic, idt, ioapic, serial};
use crate::kprintln;
use crate::sched::wait::WaitQueue;
use crate::time;

/// IDT vector used for the console receive interrupt (outside the timer, IPI and driver ranges).
pub const RX_VECTOR: u8 = 0x22;
/// ISA IRQ of COM1.
const ISA_IRQ: u8 = 4;
/// Maximum bytes drained per interrupt or tick; the 16-byte FIFO never holds that many.
const MAX_DRAIN: usize = 256;
/// Minimum interval between overrun log lines.
const DROP_LOG_INTERVAL_NS: u64 = 1_000_000_000;

/// Received bytes not yet delivered to a reader.
static RX: ringbuf::RxRing = ringbuf::RxRing::new();
/// Readers blocked in `DEBUG_READ`.
static WAITERS: WaitQueue = WaitQueue::new();
/// True when the timer tick polls the UART instead of using the IOAPIC line.
static POLLING: AtomicBool = AtomicBool::new(false);
/// Monotonic time of the last overrun log line.
static DROP_LOG_AT: AtomicU64 = AtomicU64::new(0);
/// Dropped count already reported.
static DROP_REPORTED: AtomicU64 = AtomicU64::new(0);

/// Route COM1 receive to [`RX_VECTOR`], or fall back to timer-tick polling.
///
/// Called once at boot, after the IDT and IOAPIC are initialised and before interrupts are
/// enabled.  The redirection entry is programmed masked by `ioapic::route` and unmasked here.
pub fn init() {
    let gsi = ioapic::gsi_for_isa(ISA_IRQ);
    let (active_low, level) = acpi::info()
        .override_for_isa(ISA_IRQ)
        .unwrap_or((false, false));

    if !idt::set_handler(u16::from(RX_VECTOR), rx_isr_address()) {
        serial::disable_rx_interrupt();
        POLLING.store(true, Ordering::Release);
        kprintln!("console: rx polling (vector {RX_VECTOR:#x} unavailable)");
        return;
    }
    match ioapic::route(gsi, RX_VECTOR, apic::id(), active_low, level) {
        Ok(()) => match ioapic::unmask(gsi) {
            Ok(()) => {
                serial::enable_rx_interrupt();
                kprintln!(
                    "console: rx irq {ISA_IRQ} gsi {gsi} vector {RX_VECTOR:#x} ({})",
                    if active_low {
                        "active-low"
                    } else {
                        "active-high"
                    }
                );
            }
            Err(error) => {
                serial::disable_rx_interrupt();
                POLLING.store(true, Ordering::Release);
                kprintln!("console: rx polling ({error})");
            }
        },
        Err(error) => {
            serial::disable_rx_interrupt();
            POLLING.store(true, Ordering::Release);
            kprintln!("console: rx polling ({error})");
        }
    }
}

/// True when the receive path is polled from the timer tick.
pub fn polling() -> bool {
    POLLING.load(Ordering::Acquire)
}

/// Append one received byte.
///
/// Called by the interrupt handler and by the polling fallback.  Lock-free and allocation-free.
pub fn push_byte(byte: u8) {
    RX.push(byte);
}

/// Drain the UART into the ring, returning true when at least one byte arrived.
fn drain() -> bool {
    let mut any = false;
    let mut count = 0usize;
    while count < MAX_DRAIN {
        let Some(byte) = serial::read_rx() else {
            break;
        };
        push_byte(byte);
        any = true;
        count = count.saturating_add(1);
    }
    any
}

/// Wake one reader (the wait queue is safe from interrupt context).
fn notify() {
    let _ = WAITERS.wake_one(0, 0);
}

/// Timer-tick fallback: drain the UART when no IOAPIC route is available.
pub fn poll() {
    if !polling() {
        return;
    }
    if drain() {
        notify();
    }
}

/// Drain up to `out.len()` received bytes, returning the count.
pub fn read_into(out: &mut [u8]) -> usize {
    RX.take(out)
}

/// True when at least one received byte is waiting.
pub fn available() -> bool {
    !RX.is_empty()
}

/// The readers' wait queue (`DEBUG_READ`).
pub fn waiters() -> &'static WaitQueue {
    &WAITERS
}

/// Total bytes dropped by ring overrun.
pub fn dropped() -> u64 {
    RX.dropped()
}

/// Report overrun drops from process context, at most once per second.
///
/// The interrupt handler must not take the serial log lock; the reader path reports the
/// accumulated count instead.
pub fn log_dropped() {
    let dropped = self::dropped();
    if dropped == DROP_REPORTED.load(Ordering::Relaxed) {
        return;
    }
    let now = time::now_ns();
    let last = DROP_LOG_AT.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < DROP_LOG_INTERVAL_NS {
        return;
    }
    DROP_LOG_AT.store(now, Ordering::Relaxed);
    DROP_REPORTED.store(dropped, Ordering::Relaxed);
    kprintln!("console: rx dropped {dropped}");
}

core::arch::global_asm!(
    r#"
    .section .text
    .global console_rx_isr
    .type console_rx_isr, @function
console_rx_isr:
    test byte ptr [rsp+8], 3
    jz 1f
    swapgs
1:
    cld
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push rbp
    mov rbp, rsp
    mov rdi, rbp
    and rsp, -16
    call console_rx_interrupt
    mov rsp, rbp
    pop rbp
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    test byte ptr [rsp+8], 3
    jz 2f
    swapgs
2:
    iretq
    .size console_rx_isr, . - console_rx_isr
"#
);

unsafe extern "C" {
    fn console_rx_isr();
}

fn rx_isr_address() -> u64 {
    console_rx_isr as *const () as usize as u64
}

/// Rust side of the receive stub.
///
/// Runs with interrupts masked by the CPU: no allocation, no blocking, no serial logging.
#[unsafe(no_mangle)]
extern "C" fn console_rx_interrupt(_rsp: u64) {
    drain();
    notify();
    apic::eoi();
}
