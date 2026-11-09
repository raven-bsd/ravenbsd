// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! COM1 (16550) polled serial output.

use core::arch::asm;
use core::fmt;

/// Base I/O port of COM1.
const COM1: u16 = 0x3f8;
/// Line status register offset.
const LSR: u16 = COM1 + 5;
/// Transmitter holding register empty.
const LSR_THRE: u8 = 1 << 5;
/// Maximum polls per byte before dropping it.
const TX_SPIN_LIMIT: u32 = 100_000;

/// A writer for COM1. The type carries no state; use [`init`] once and then write through it.
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

/// Initialise COM1 for 115200 baud, 8 data bits, no parity, one stop bit, FIFO on.
pub fn init() {
    // SAFETY: COM1 is the legacy 16550 UART; these are the standard polled-mode programming
    // writes for the legacy port.
    unsafe {
        outb(COM1 + 1, 0x00); // disable interrupts
        outb(COM1 + 3, 0x80); // enable divisor latch
        outb(COM1, 0x01); // divisor 1 = 115200 baud
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // 8N1
        outb(COM1 + 2, 0xc7); // enable and clear FIFO, 14-byte threshold
        outb(COM1 + 4, 0x03); // DTR and RTS
    }
}

fn write_byte(byte: u8) {
    // SAFETY: polling LSR bit 5 and writing THR is the polled transmit sequence on COM1.
    unsafe {
        for _ in 0..TX_SPIN_LIMIT {
            if inb(LSR) & LSR_THRE != 0 {
                outb(COM1, byte);
                return;
            }
        }
    }
}

/// Write raw bytes to COM1, translating LF to CRLF.
pub fn write_bytes(bytes: &[u8]) {
    for &byte in bytes {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: `out` writes the given I/O port; callers pass a valid UART register.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `in` reads the given I/O port; callers pass a valid UART register.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}
