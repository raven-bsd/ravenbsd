// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! COM1 (16550) polled output for the test kernel.

use core::arch::asm;
use core::fmt::{self, Write};

const COM1: u16 = 0x3f8;
const LSR: u16 = COM1 + 5;
const LSR_THRE: u8 = 1 << 5;

/// Initialise COM1 for 115200 8N1.
pub fn init() {
    // SAFETY: standard 16550 polled-mode initialisation on the legacy COM1 port.
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x01);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xc7);
        outb(COM1 + 4, 0x03);
    }
}

/// Write one byte, waiting for the transmitter to become empty.
pub fn write_byte(byte: u8) {
    // SAFETY: polling LSR bit 5 then writing THR is the polled transmit sequence.
    unsafe {
        while inb(LSR) & LSR_THRE == 0 {}
        outb(COM1, byte);
    }
}

/// Write formatted output to COM1, without a prefix.
pub fn print(args: fmt::Arguments<'_>) {
    let mut writer = Writer;
    let _ = writer.write_fmt(args);
}

struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for &byte in text.as_bytes() {
            write_byte(byte);
        }
        Ok(())
    }
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: `out` writes the given I/O port; callers pass UART registers.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `in` reads the given I/O port; callers pass UART registers.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}
