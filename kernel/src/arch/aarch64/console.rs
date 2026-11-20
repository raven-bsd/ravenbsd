// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 PL011 receive console: RX ring, wait queue and GIC route.
//!
//! The PL011 receive interrupt drains the FIFO into the same lock-free ring and `WaitQueue`
//! contract as the x86-64 console. `DEBUG_READ` consumes the ring; the interrupt handler never
//! allocates, logs or blocks. The UART line is a GIC SPI on both QEMU `virt` (33) and the Pi 4
//! (153); the boot path supplies the id and this module configures the level/edge route.

#[path = "../../../../crates/raven-ringbuf/src/lib.rs"]
mod ringbuf;

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::arch::aarch64::{fdt, gic, irq, serial};
use crate::kprintln;
use crate::sched::wait::WaitQueue;
use crate::time;

/// Maximum bytes drained per interrupt; the PL011 FIFO is much smaller.
const MAX_DRAIN: usize = 256;
/// Minimum interval between overrun log lines.
const DROP_LOG_INTERVAL_NS: u64 = 1_000_000_000;

static RX: ringbuf::RxRing = ringbuf::RxRing::new();
static WAITERS: WaitQueue = WaitQueue::new();
static DROPPED_REPORTED: AtomicU64 = AtomicU64::new(0);
static DROP_LOG_AT: AtomicU64 = AtomicU64::new(0);
static IRQ_LINE: AtomicU32 = AtomicU32::new(0);

/// Route the PL011 receive interrupt through the GIC and enable it.
///
/// Called after GIC init and before interrupts are unmasked. A failure keeps the UART output
/// path and logs the reason; `DEBUG_READ` simply remains without bytes.
pub fn init(uart_irq: u32) {
    let route = fdt::spi_route(uart_irq);
    let configured = gic::configure_spi(uart_irq, route.level)
        .and_then(|()| gic::set_target(uart_irq, 0))
        .and_then(|()| gic::set_priority(uart_irq, 0xa0))
        .and_then(|()| gic::enable(uart_irq));
    match configured {
        Ok(()) => {
            IRQ_LINE.store(uart_irq, Ordering::Release);
            irq::set_uart_irq(uart_irq);
            serial::enable_rx_interrupt();
            kprintln!(
                "console: PL011 rx irq {uart_irq} ({})",
                if route.level { "level" } else { "edge" }
            );
        }
        Err(error) => {
            serial::disable_rx_interrupt();
            kprintln!("console: PL011 rx unavailable: {error}");
        }
    }
}

/// The configured UART interrupt id, or zero.
#[must_use]
pub fn irq_line() -> u32 {
    IRQ_LINE.load(Ordering::Acquire)
}

/// Drain the PL011 FIFO into the ring and wake one reader.
///
/// The pending RX/RT cause is cleared before the drain so a stale or duplicate interrupt cannot
/// spin. Bytes that arrive during the drain set the cause again, and the level-triggered GIC
/// re-enters after EOI. When the FIFO is empty at the end, clear once more to drop a
/// receive-timeout cause belonging to the bytes just consumed. If the FIFO still holds data
/// (the drain bound was reached), leave the cause set so the remaining bytes are delivered.
pub fn handle_irq() {
    serial::clear_rx_interrupt();
    let mut any = false;
    let mut count = 0usize;
    while count < MAX_DRAIN {
        let Some(byte) = serial::read_rx() else {
            break;
        };
        RX.push(byte);
        any = true;
        count = count.saturating_add(1);
    }
    if !serial::rx_available() {
        serial::clear_rx_interrupt();
    }
    if any {
        let _ = WAITERS.wake_one(0, 0);
    }
}

/// Feed one byte from a non-UART source (keyboard) into the same ring.
pub fn feed_byte(byte: u8) {
    RX.push(byte);
    let _ = WAITERS.wake_one(0, 0);
}

/// Drain up to `out.len()` bytes.
pub fn read_into(out: &mut [u8]) -> usize {
    RX.take(out)
}

/// True when a received byte is waiting.
#[must_use]
pub fn available() -> bool {
    !RX.is_empty()
}

/// The `DEBUG_READ` wait queue.
pub fn waiters() -> &'static WaitQueue {
    &WAITERS
}

/// Total ring overruns.
pub fn dropped() -> u64 {
    RX.dropped()
}

/// Report ring overruns from process context, at most once a second.
pub fn log_dropped() {
    let dropped = self::dropped();
    if dropped == DROPPED_REPORTED.load(Ordering::Relaxed) {
        return;
    }
    let now = time::now_ns();
    let last = DROP_LOG_AT.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < DROP_LOG_INTERVAL_NS {
        return;
    }
    DROP_LOG_AT.store(now, Ordering::Relaxed);
    DROPPED_REPORTED.store(dropped, Ordering::Relaxed);
    kprintln!("console: rx dropped {dropped}");
}
