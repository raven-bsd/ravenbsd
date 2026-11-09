// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! COM1 (16550) polled serial output, mirrored to the UEFI console.

use alloc::format;
use core::arch::asm;
use core::fmt;

/// Base I/O port of the legacy COM1 UART.
const COM1: u16 = 0x3f8;
/// Line status register offset.
const LSR: u16 = COM1 + 5;
/// Transmitter holding register empty.
const LSR_THRE: u8 = 1 << 5;

/// Initialise COM1 for 115200 baud, 8 data bits, no parity, one stop bit.
pub fn init() {
    // SAFETY: COM1 is the legacy 16550 UART on x86-64; these are the standard polled-mode
    // initialisation writes from the UART programming sequence.
    unsafe {
        outb(COM1 + 1, 0x00); // disable interrupts
        outb(COM1 + 3, 0x80); // enable divisor latch
        outb(COM1, 0x01); // divisor 1 = 115200 baud
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // 8N1
        outb(COM1 + 2, 0xc7); // enable and clear FIFO
        outb(COM1 + 4, 0x03); // DTR and RTS
    }
}

/// Write one byte to COM1, waiting until the transmitter is empty.
pub fn write_byte(byte: u8) {
    // SAFETY: reading the line status register and writing the data register is the polled UART
    // transmit sequence. COM1 exists on the x86-64 platforms this loader targets.
    unsafe {
        while inb(LSR) & LSR_THRE == 0 {}
        outb(COM1, byte);
    }
}

/// Print one `loader: `-prefixed line to COM1 and then to the UEFI console.
///
/// The line is rendered once and written to each console as a whole. On QEMU both consoles can
/// be the same serial device, so the line is expected to appear twice there.
pub fn log(args: fmt::Arguments<'_>) {
    let line = format!("loader: {args}\n");
    for &byte in line.as_bytes() {
        write_byte(byte);
    }
    uefi::print!("{line}");
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: `out` writes to the given I/O port; the caller passes a valid UART register.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `in` reads from the given I/O port; the caller passes a valid UART register.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}
