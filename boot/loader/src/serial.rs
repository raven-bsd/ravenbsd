// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Polled serial output, mirrored to the UEFI console before boot services exit.
//!
//! x86-64 uses the legacy COM1 16550; aarch64 uses the QEMU `virt` PL011 at `0x0900_0000`. The UEFI
//! ConOut mirror is architecture-neutral.

#[cfg(target_arch = "x86_64")]
use core::arch::asm;
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicBool, Ordering};

/// Base I/O port of the legacy COM1 UART.
#[cfg(target_arch = "x86_64")]
const COM1: u16 = 0x3f8;
/// Line status register offset.
#[cfg(target_arch = "x86_64")]
const LSR: u16 = COM1 + 5;
/// Transmitter holding register empty.
#[cfg(target_arch = "x86_64")]
const LSR_THRE: u8 = 1 << 5;

/// QEMU `virt` PL011 base (physical; identity-mapped before the handoff).
#[cfg(target_arch = "aarch64")]
const PL011: u64 = 0x0900_0000;
/// PL011 data register.
#[cfg(target_arch = "aarch64")]
const PL011_DR: u64 = PL011;
/// PL011 flag register.
#[cfg(target_arch = "aarch64")]
const PL011_FR: u64 = PL011 + 0x18;
/// PL011 integer baud divisor.
#[cfg(target_arch = "aarch64")]
const PL011_IBRD: u64 = PL011 + 0x24;
/// PL011 fractional baud divisor.
#[cfg(target_arch = "aarch64")]
const PL011_FBRD: u64 = PL011 + 0x28;
/// PL011 line control register.
#[cfg(target_arch = "aarch64")]
const PL011_LCR_H: u64 = PL011 + 0x2c;
/// PL011 control register.
#[cfg(target_arch = "aarch64")]
const PL011_CR: u64 = PL011 + 0x30;
/// PL011 interrupt clear register.
#[cfg(target_arch = "aarch64")]
const PL011_ICR: u64 = PL011 + 0x44;
/// Transmit FIFO full.
#[cfg(target_arch = "aarch64")]
const PL011_FR_TXFF: u32 = 1 << 5;

static CONOUT: AtomicBool = AtomicBool::new(false);

/// Initialise COM1 and enable the UEFI ConOut mirror.
pub fn init() {
    init_serial_only();
    CONOUT.store(true, Ordering::Release);
}

/// Initialise the UART without touching UEFI services (safe after `ExitBootServices` and in
/// panics).
pub fn init_serial_only() {
    #[cfg(target_arch = "x86_64")]
    {
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
    #[cfg(target_arch = "aarch64")]
    {
        // QEMU virt PL011 clock is 24 MHz: 24e6 / (16 * 115200) = 13.0208 -> IBRD 13, FBRD 1.
        // SAFETY: the PL011 frame is MMIO and identity-mapped by UEFI before the handoff.
        unsafe {
            write32(PL011_CR, 0);
            write32(PL011_IBRD, 13);
            write32(PL011_FBRD, 1);
            write32(PL011_LCR_H, 0x70); // 8 bits, no parity, one stop, FIFOs
            write32(PL011_ICR, 0x7ff);
            write32(PL011_CR, 0x301); // UARTEN | TXE | RXE
        }
    }
}

/// Stop mirroring log lines to the UEFI console. Call before `ExitBootServices`.
pub fn disable_conout() {
    CONOUT.store(false, Ordering::Release);
}

/// Write one byte, waiting until the transmitter is empty.
pub fn write_byte(byte: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: reading the line status register and writing the data register is the polled
        // UART transmit sequence. COM1 exists on the x86-64 platforms this loader targets.
        unsafe {
            while inb(LSR) & LSR_THRE == 0 {}
            outb(COM1, byte);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the PL011 frame is MMIO; polling TXFF and writing DR is the polled sequence.
        unsafe {
            while read32(PL011_FR) & PL011_FR_TXFF != 0 {}
            write32(PL011_DR, u32::from(byte));
        }
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

#[cfg(target_arch = "aarch64")]
unsafe fn read32(address: u64) -> u32 {
    // SAFETY: the caller passes a mapped PL011 register.
    unsafe { (address as *const u32).read_volatile() }
}

#[cfg(target_arch = "aarch64")]
unsafe fn write32(address: u64, value: u32) {
    // SAFETY: the caller passes a mapped PL011 register.
    unsafe { (address as *mut u32).write_volatile(value) };
}

#[cfg(target_arch = "x86_64")]
unsafe fn outb(port: u16, value: u8) {
    // SAFETY: `out` writes to the given I/O port; the caller passes a valid UART register.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `in` reads from the given I/O port; the caller passes a valid UART register.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}
