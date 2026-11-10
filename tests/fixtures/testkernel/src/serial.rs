// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Polled serial output for the test kernel.
//!
//! x86-64 uses COM1; aarch64 uses the QEMU `virt` PL011 at physical `0x0900_0000`, reached through
//! the loader's HHDM (attribute 0, device-nGnRnE).

#[cfg(target_arch = "x86_64")]
use core::arch::asm;
use core::fmt::{self, Write};

#[cfg(target_arch = "x86_64")]
const COM1: u16 = 0x3f8;
#[cfg(target_arch = "x86_64")]
const LSR: u16 = COM1 + 5;
#[cfg(target_arch = "x86_64")]
const LSR_THRE: u8 = 1 << 5;

#[cfg(target_arch = "aarch64")]
const PL011: u64 = 0xFFFF_8000_0900_0000;
#[cfg(target_arch = "aarch64")]
const PL011_DR: u64 = PL011;
#[cfg(target_arch = "aarch64")]
const PL011_FR: u64 = PL011 + 0x18;
#[cfg(target_arch = "aarch64")]
const PL011_IBRD: u64 = PL011 + 0x24;
#[cfg(target_arch = "aarch64")]
const PL011_FBRD: u64 = PL011 + 0x28;
#[cfg(target_arch = "aarch64")]
const PL011_LCR_H: u64 = PL011 + 0x2c;
#[cfg(target_arch = "aarch64")]
const PL011_CR: u64 = PL011 + 0x30;
#[cfg(target_arch = "aarch64")]
const PL011_ICR: u64 = PL011 + 0x44;
#[cfg(target_arch = "aarch64")]
const PL011_FR_TXFF: u32 = 1 << 5;

/// Initialise the UART for 115200 8N1.
pub fn init() {
    #[cfg(target_arch = "x86_64")]
    {
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
    #[cfg(target_arch = "aarch64")]
    {
        // 24 MHz PL011 clock / (16 * 115200) = 13.0208 -> IBRD 13, FBRD 1.
        // SAFETY: the PL011 frame is mapped through HHDM by the loader.
        unsafe {
            write32(PL011_CR, 0);
            write32(PL011_IBRD, 13);
            write32(PL011_FBRD, 1);
            write32(PL011_LCR_H, 0x70);
            write32(PL011_ICR, 0x7ff);
            write32(PL011_CR, 0x301);
        }
    }
}

/// Write one byte, waiting for the transmitter to become empty.
pub fn write_byte(byte: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: polling LSR bit 5 then writing THR is the polled transmit sequence.
        unsafe {
            while inb(LSR) & LSR_THRE == 0 {}
            outb(COM1, byte);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the PL011 frame is MMIO; polling TXFF then writing DR is the polled sequence.
        unsafe {
            while read32(PL011_FR) & PL011_FR_TXFF != 0 {}
            write32(PL011_DR, u32::from(byte));
        }
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
    // SAFETY: `out` writes the given I/O port; callers pass UART registers.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `in` reads the given I/O port; callers pass UART registers.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    }
    value
}
