// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PL011 polled serial output for QEMU `virt` and the Raspberry Pi 4.
//!
//! The frame is reached through the HHDM with device attributes. The base and input clock are
//! configured by the boot path: QEMU `virt` defaults to `0x0900_0000` at 24 MHz, while the Pi 4
//! entry stub sets `0xFE20_1000` at 48 MHz from its FDT.

use core::fmt;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::arch::HHDM_BASE;

/// Physical base of the PL011 UART on QEMU `virt`.
const QEMU_VIRT_PL011: u64 = 0x0900_0000;
/// QEMU `virt` PL011 input clock.
const QEMU_VIRT_CLOCK: u32 = 24_000_000;
/// PL011 data register offset.
const DR_OFFSET: u64 = 0x00;
/// PL011 flag register offset.
const FR_OFFSET: u64 = 0x18;
/// PL011 integer baud-rate divisor offset.
const IBRD_OFFSET: u64 = 0x24;
/// PL011 fractional baud-rate divisor offset.
const FBRD_OFFSET: u64 = 0x28;
/// PL011 line control register offset.
const LCR_H_OFFSET: u64 = 0x2c;
/// PL011 control register offset.
const CR_OFFSET: u64 = 0x30;
/// PL011 interrupt clear register offset.
const ICR_OFFSET: u64 = 0x44;
/// PL011 interrupt mask set/clear register offset.
const IMSC_OFFSET: u64 = 0x38;
/// PL011 interrupt FIFO level select register offset.
const IFLS_OFFSET: u64 = 0x34;
/// Receive FIFO not empty (active-low: clear means data is available).
const FR_RXFE: u32 = 1 << 4;
/// Receive interrupt mask.
const RXIM: u32 = 1 << 4;
/// Receive-timeout interrupt mask: asserts after the receive timeout while the FIFO is non-empty
/// and below the RX trigger level, so a single short byte is not held until the FIFO fills.
const RTIM: u32 = 1 << 6;
/// RXIFLSEL value selecting a 1/8-full receive trigger (two bytes on the 16-byte FIFO).
const RXIFLSEL_SHORT: u32 = 0;
/// Transmit FIFO full.
const FR_TXFF: u32 = 1 << 5;
/// Maximum polls per byte before dropping it.
const TX_SPIN_LIMIT: u32 = 100_000;
/// Default baud rate.
const BAUD: u64 = 115_200;

static BASE: AtomicU64 = AtomicU64::new(HHDM_BASE + QEMU_VIRT_PL011);
static CLOCK_HZ: AtomicU32 = AtomicU32::new(QEMU_VIRT_CLOCK);

/// A writer for the PL011. The type carries no state; use [`init`] once and then write through it.
pub struct Serial;

impl fmt::Write for Serial {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        // HDMI1: mirror every kernel line to the HDMI console when it is live. The UART write
        // below is unconditional, so a missing/failed display console never changes the serial
        // transcript or the boot/login gate.
        #[cfg(raven_pi4)]
        super::fb::write_text(text.as_bytes());
        for &byte in text.as_bytes() {
            if byte == b'\n' {
                write_byte(b'\r');
            }
            write_byte(byte);
        }
        Ok(())
    }
}

/// Point the driver at a physical PL011 frame (through the HHDM) and its input clock.
#[allow(dead_code)] // The Pi 4 entry stub sets the Pi's UART; the UEFI path keeps the default.
pub fn set_uart(physical_base: u64, clock_hz: u32) {
    if physical_base != 0 && clock_hz != 0 {
        BASE.store(HHDM_BASE.saturating_add(physical_base), Ordering::Release);
        CLOCK_HZ.store(clock_hz, Ordering::Release);
    }
}

/// Initialise the PL011 for 115200 baud, 8 data bits, no parity, one stop bit, FIFO on.
pub fn init() {
    let base = BASE.load(Ordering::Acquire);
    let clock = u64::from(CLOCK_HZ.load(Ordering::Acquire));
    let divider = 16 * BAUD;
    let divisor = clock / divider;
    let remainder = clock % divider;
    let fractional = remainder.wrapping_mul(64).wrapping_add(divider / 2) / divider;
    // SAFETY: the PL011 frame is mapped through the HHDM by the boot path; these are its
    // registers.
    unsafe {
        write32(base, CR_OFFSET, 0);
        write32(base, IBRD_OFFSET, u32::try_from(divisor).unwrap_or(0));
        write32(base, FBRD_OFFSET, u32::try_from(fractional).unwrap_or(0));
        write32(base, LCR_H_OFFSET, 0x70); // 8 bits, FIFOs enabled, no parity
        write32(base, IFLS_OFFSET, RXIFLSEL_SHORT); // short RX trigger, timeout fills the gap
        write32(base, IMSC_OFFSET, 0); // no RX interrupt until the console is routed
        write32(base, ICR_OFFSET, 0x7ff); // clear pending interrupts
        write32(base, CR_OFFSET, 0x301); // UART enable, TX enable, RX enable
    }
}

/// Pop one received byte when the PL011 RX FIFO is not empty.
pub fn read_rx() -> Option<u8> {
    let base = BASE.load(Ordering::Acquire);
    // SAFETY: the PL011 frame is mapped through the HHDM; FR is a register access.
    let flags = unsafe { read32(base, FR_OFFSET) };
    if flags & FR_RXFE != 0 {
        None
    } else {
        // SAFETY: DR is a register in the same mapped PL011 frame and FR_RXFE is clear.
        Some(unsafe { read32(base, DR_OFFSET) } as u8)
    }
}

/// True when the RX FIFO holds at least one byte; the byte is not consumed.
pub fn rx_available() -> bool {
    let base = BASE.load(Ordering::Acquire);
    // SAFETY: the PL011 frame is mapped through the HHDM; FR is a register access.
    let flags = unsafe { read32(base, FR_OFFSET) };
    flags & FR_RXFE == 0
}

/// Enable the PL011 receive and receive-timeout interrupts.
pub fn enable_rx_interrupt() {
    let base = BASE.load(Ordering::Acquire);
    // SAFETY: the PL011 frame is mapped through the HHDM; IMSC is its mask register.
    unsafe {
        write32(base, IMSC_OFFSET, RXIM | RTIM);
    }
}

/// Disable the PL011 receive interrupt.
pub fn disable_rx_interrupt() {
    let base = BASE.load(Ordering::Acquire);
    // SAFETY: the PL011 frame is mapped through the HHDM; IMSC is its mask register.
    unsafe {
        write32(base, IMSC_OFFSET, 0);
    }
}

/// Clear a pending receive or receive-timeout interrupt after draining the FIFO.
pub fn clear_rx_interrupt() {
    let base = BASE.load(Ordering::Acquire);
    // SAFETY: the PL011 frame is mapped through the HHDM; ICR is write-one-to-clear.
    unsafe {
        write32(base, ICR_OFFSET, RXIM | RTIM);
    }
}

/// Write one byte, waiting for the transmit FIFO to accept it.
pub fn write_byte(byte: u8) {
    let base = BASE.load(Ordering::Acquire);
    for _ in 0..TX_SPIN_LIMIT {
        // SAFETY: the PL011 frame is mapped through the HHDM; reading FR is a register read.
        let full = unsafe { read32(base, FR_OFFSET) } & FR_TXFF != 0;
        if !full {
            // SAFETY: the transmit FIFO has room; writing DR queues the byte.
            unsafe {
                write32(base, DR_OFFSET, u32::from(byte));
            }
            return;
        }
    }
}

/// Write raw bytes to the PL011, translating LF to CRLF.
#[allow(dead_code)] // API parity with x86-64; the S14 DEBUG_WRITE path uses it.
pub fn write_bytes(bytes: &[u8]) {
    // FB1: the S14 DEBUG_WRITE path is how consd copies getty/login output. Mirror that stream
    // to the same HDMI text console as the kernel log; UART remains the unconditional sink.
    #[cfg(raven_pi4)]
    super::fb::write_text(bytes);
    for &byte in bytes {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}

unsafe fn read32(base: u64, offset: u64) -> u32 {
    // SAFETY: the caller passes a mapped PL011 register address.
    unsafe { (base.saturating_add(offset) as *const u32).read_volatile() }
}

unsafe fn write32(base: u64, offset: u64, value: u32) {
    // SAFETY: the caller passes a mapped PL011 register address.
    unsafe {
        (base.saturating_add(offset) as *mut u32).write_volatile(value);
    }
}
