// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PL011 polled serial output for the QEMU `virt` UART.
//!
//! The loader maps the PL011 frame into the HHDM as device-nGnRnE memory (S12), so the kernel only
//! adds the direct-map base to the physical address and programs the same 115200 8N1 sequence as
//! `tests/fixtures/testkernel/src/serial.rs`. Pi 4 uses the same driver with its own base (S15).

use core::fmt;

use crate::arch::HHDM_BASE;

/// Physical base of the PL011 UART on QEMU `virt`.
const PL011_PHYS: u64 = 0x0900_0000;
/// PL011 data register.
const DR: u64 = HHDM_BASE + PL011_PHYS;
/// PL011 flag register.
const FR: u64 = DR + 0x18;
/// PL011 integer baud-rate divisor.
const IBRD: u64 = DR + 0x24;
/// PL011 fractional baud-rate divisor.
const FBRD: u64 = DR + 0x28;
/// PL011 line control register.
const LCR_H: u64 = DR + 0x2c;
/// PL011 control register.
const CR: u64 = DR + 0x30;
/// PL011 interrupt clear register.
const ICR: u64 = DR + 0x44;
/// Transmit FIFO full.
const FR_TXFF: u32 = 1 << 5;
/// Maximum polls per byte before dropping it.
const TX_SPIN_LIMIT: u32 = 100_000;

/// A writer for the PL011. The type carries no state; use [`init`] once and then write through it.
pub struct Serial;

impl fmt::Write for Serial {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for &byte in text.as_bytes() {
            if byte == b'\n' {
                write_byte(b'\r');
            }
            write_byte(byte);
        }
        Ok(())
    }
}

/// Initialise the PL011 for 115200 baud, 8 data bits, no parity, one stop bit, FIFO on.
pub fn init() {
    // 24 MHz PL011 clock / (16 * 115200) = 13.0208 -> IBRD 13, FBRD 1.
    // SAFETY: the PL011 frame is mapped through the HHDM by the loader; these are its registers.
    unsafe {
        write32(CR, 0);
        write32(IBRD, 13);
        write32(FBRD, 1);
        write32(LCR_H, 0x70); // 8 bits, FIFOs enabled, no parity
        write32(ICR, 0x7ff); // clear pending interrupts
        write32(CR, 0x301); // UART enable, TX enable, RX enable
    }
}

/// Write one byte, waiting for the transmit FIFO to accept it.
pub fn write_byte(byte: u8) {
    for _ in 0..TX_SPIN_LIMIT {
        // SAFETY: the PL011 frame is mapped through the HHDM; reading FR is a register read.
        let full = unsafe { read32(FR) } & FR_TXFF != 0;
        if !full {
            // SAFETY: the transmit FIFO has room; writing DR queues the byte.
            unsafe {
                write32(DR, u32::from(byte));
            }
            return;
        }
    }
}

/// Write raw bytes to the PL011, translating LF to CRLF.
#[allow(dead_code)] // API parity with x86-64; the S14 DEBUG_WRITE path uses it.
pub fn write_bytes(bytes: &[u8]) {
    for &byte in bytes {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}

unsafe fn read32(address: u64) -> u32 {
    // SAFETY: the caller passes a mapped PL011 register address.
    unsafe { (address as *const u32).read_volatile() }
}

unsafe fn write32(address: u64, value: u32) {
    // SAFETY: the caller passes a mapped PL011 register address.
    unsafe { (address as *mut u32).write_volatile(value) };
}
