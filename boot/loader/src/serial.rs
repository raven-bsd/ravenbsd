// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! COM1 (16550) polled serial output, mirrored to the UEFI console before boot services exit.

use core::arch::asm;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

/// Base I/O port of the legacy COM1 UART.
const COM1: u16 = 0x3f8;
/// Line status register offset.
const LSR: u16 = COM1 + 5;
/// Transmitter holding register empty.
const LSR_THRE: u8 = 1 << 5;

static CONOUT: AtomicBool = AtomicBool::new(false);

/// Initialise COM1 and enable the UEFI ConOut mirror.
pub fn init() {
    init_serial_only();
    CONOUT.store(true, Ordering::Release);
}

/// Initialise COM1 without touching UEFI services (safe after `ExitBootServices` and in panics).
pub fn init_serial_only() {
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

/// Stop mirroring log lines to the UEFI console. Call before `ExitBootServices`.
pub fn disable_conout() {
    CONOUT.store(false, Ordering::Release);
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

/// Print one `loader: `-prefixed line to COM1 and, before boot services exit, to ConOut.
///
/// The line is rendered into a fixed stack buffer; no allocation happens here, so this remains
/// valid after `ExitBootServices`.
pub fn log(args: fmt::Arguments<'_>) {
    let mut line = LineBuffer::new();
    let _ = write!(&mut line, "loader: {args}");
    line.push(b'\n');

    for &byte in &line.bytes[..line.len] {
        write_byte(byte);
    }
    if CONOUT.load(Ordering::Acquire)
        && let Ok(text) = core::str::from_utf8(&line.bytes[..line.len])
    {
        uefi::print!("{text}");
    }
}

struct LineBuffer {
    bytes: [u8; 512],
    len: usize,
}

impl LineBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; 512],
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
    }
}

impl Write for LineBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for &byte in text.as_bytes() {
            self.push(byte);
        }
        Ok(())
    }
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
