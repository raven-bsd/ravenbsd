// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Interrupt descriptor table.

use core::cell::UnsafeCell;
use core::mem::size_of;

use crate::arch::x86_64::exception;
use crate::arch::x86_64::gdt::KERNEL_CS;

/// Number of IDT entries.
pub const IDT_ENTRIES: usize = 256;

/// One 16-byte x86-64 interrupt gate.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

const _: () = assert!(size_of::<IdtEntry>() == 16);

impl IdtEntry {
    const ZERO: IdtEntry = IdtEntry {
        offset_low: 0,
        selector: 0,
        ist: 0,
        type_attr: 0,
        offset_mid: 0,
        offset_high: 0,
        reserved: 0,
    };

    const fn gate(handler: u64, selector: u16, dpl: u8, ist: u8) -> IdtEntry {
        IdtEntry {
            offset_low: handler as u16,
            selector,
            ist: ist & 0x7,
            type_attr: 0x8e | ((dpl & 0x3) << 5),
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

struct IdtCell(UnsafeCell<[IdtEntry; IDT_ENTRIES]>);

// SAFETY: the IDT is filled once during single-CPU early init and then only read by the CPU.
unsafe impl Sync for IdtCell {}

static IDT: IdtCell = IdtCell(UnsafeCell::new([IdtEntry::ZERO; IDT_ENTRIES]));

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

/// Fill and load the 256-entry IDT.
pub fn init() {
    let idt = IDT.0.get();
    // SAFETY: the IDT static is owned by the kernel and written before `lidt`.
    unsafe {
        let table = &mut *idt;
        for vector in 0..IDT_ENTRIES {
            let handler = exception::isr_address(vector as u16);
            let dpl = if vector == 3 || vector == 4 { 3 } else { 0 };
            let ist = match vector {
                2 => 2,  // NMI
                8 => 1,  // double fault
                18 => 3, // machine check
                _ => 0,
            };
            if let Some(entry) = table.get_mut(vector) {
                *entry = IdtEntry::gate(handler, KERNEL_CS, dpl, ist);
            }
        }
    }

    let pointer = DescriptorTablePointer {
        limit: size_of::<[IdtEntry; IDT_ENTRIES]>().saturating_sub(1) as u16,
        base: idt as usize as u64,
    };
    // SAFETY: the pointer describes the initialized static IDT.
    unsafe {
        core::arch::asm!(
            "lidt [{}]",
            in(reg) &pointer,
            options(nostack, preserves_flags),
        );
    }
}
