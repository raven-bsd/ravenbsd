// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Global descriptor table, TSS and IST stacks.
//!
//! The GDT is deliberately fixed at seven entries so the `STAR` arithmetic used by `sysret` in
//! S08 is stable: `STAR[63:48] = 0x13` yields `SS = 0x1B`, `CS = 0x23`.
//!
//! # Per-CPU TSS requirement
//!
//! The kernel currently has one TSS shared by every CPU, and `set_rsp0` is called only on the
//! ring-3 entry path, which the current execution model pins to CPU 0.  `rsp0` is architectural
//! per-CPU state: before user threads run on application processors, every CPU needs its own TSS
//! descriptor (and `tr` load), its own `rsp0` slot and its own IST stacks.  APs today run kernel
//! threads only and never load a user `rsp0`.

use core::cell::UnsafeCell;
use core::mem::size_of;

/// Kernel code selector.
pub const KERNEL_CS: u16 = 0x08;
/// Kernel data selector.
pub const KERNEL_SS: u16 = 0x10;
/// User data selector.
pub const USER_SS: u16 = 0x18 | 3;
/// User code selector.
pub const USER_CS: u16 = 0x20 | 3;
/// TSS selector (two GDT slots).
pub const TSS_SELECTOR: u16 = 0x28;
/// `STAR[63:48]`; `sysret` derives SS from +8 and CS from +16.
pub const STAR_USER_BASE: u16 = 0x10 | 3;

const GDT_ENTRIES: usize = 7;
const IST_SIZE: usize = 16 * 1024;

const _: () = {
    assert!(KERNEL_CS == 0x08);
    assert!(KERNEL_SS == 0x10);
    assert!(USER_SS == 0x1B);
    assert!(USER_CS == 0x23);
    assert!(STAR_USER_BASE == 0x13);
    assert!((STAR_USER_BASE as u64 + 8) & 0xffff == USER_SS as u64);
    assert!((STAR_USER_BASE as u64 + 16) & 0xffff == USER_CS as u64);
    assert!(size_of::<Tss>() == 0x68);
};

/// Encode one 8-byte code/data descriptor.
const fn entry(base: u64, limit: u32, access: u8, flags: u8) -> u64 {
    (limit as u64 & 0xffff)
        | ((base & 0xffff) << 16)
        | (((base >> 16) & 0xff) << 32)
        | ((access as u64) << 40)
        | (((limit as u64 >> 16) & 0xf) << 48)
        | (((flags & 0xf) as u64) << 52)
        | (((base >> 24) & 0xff) << 56)
}

const KERNEL_CODE_ENTRY: u64 = entry(0, 0x000f_ffff, 0x9a, 0xa);
const KERNEL_DATA_ENTRY: u64 = entry(0, 0x000f_ffff, 0x92, 0xc);
const USER_DATA_ENTRY: u64 = entry(0, 0x000f_ffff, 0xf2, 0xc);
const USER_CODE_ENTRY: u64 = entry(0, 0x000f_ffff, 0xfa, 0xa);

const _: () = assert!(KERNEL_CODE_ENTRY == 0x00af_9a00_0000_ffff);
const _: () = assert!(KERNEL_DATA_ENTRY == 0x00cf_9200_0000_ffff);
const _: () = assert!(USER_DATA_ENTRY == 0x00cf_f200_0000_ffff);
const _: () = assert!(USER_CODE_ENTRY == 0x00af_fa00_0000_ffff);

/// One TSS in the architectural 64-bit layout (104 bytes).
#[repr(C, packed(4))]
struct Tss {
    reserved_1: u32,
    rsp: [u64; 3],
    reserved_2: u64,
    ist: [u64; 7],
    reserved_3: u64,
    reserved_4: u16,
    iomap_base: u16,
}

impl Tss {
    const fn new() -> Self {
        Self {
            reserved_1: 0,
            rsp: [0; 3],
            reserved_2: 0,
            ist: [0; 7],
            reserved_3: 0,
            reserved_4: 0,
            iomap_base: size_of::<Tss>() as u16,
        }
    }
}

#[repr(C, align(16))]
struct Stack(UnsafeCell<[u8; IST_SIZE]>);

// SAFETY: the CPU writes the stack while delivering an exception; no Rust code holds a reference
// into it at the same time.
unsafe impl Sync for Stack {}

impl Stack {
    const fn new() -> Self {
        Self(UnsafeCell::new([0; IST_SIZE]))
    }

    fn top(&'static self) -> u64 {
        (self.0.get() as u64).saturating_add(IST_SIZE as u64)
    }
}

static TSS: TssCell = TssCell(UnsafeCell::new(Tss::new()));
static GDT: GdtCell = GdtCell(UnsafeCell::new([0u64; GDT_ENTRIES]));
#[unsafe(link_section = ".bss.__ist")]
static IST1: Stack = Stack::new();
#[unsafe(link_section = ".bss.__ist")]
static IST2: Stack = Stack::new();
#[unsafe(link_section = ".bss.__ist")]
static IST3: Stack = Stack::new();

struct TssCell(UnsafeCell<Tss>);
struct GdtCell(UnsafeCell<[u64; GDT_ENTRIES]>);

// SAFETY: the TSS is written during early init and by `set_rsp0`.  `set_rsp0` runs only on the
// CPU that enters ring 3 (CPU 0 in the current model), so there is one writer at a time.  The
// per-CPU TSS requirement above must be met before user threads run on APs.
unsafe impl Sync for TssCell {}
// SAFETY: the GDT is written once during early init and then only read by the CPU.
unsafe impl Sync for GdtCell {}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

/// Initialise the GDT, TSS and IST stacks and load them.
pub fn init() {
    let tss = TSS.0.get();
    // SAFETY: the TSS static is owned by the kernel and has not been published yet.
    unsafe {
        core::ptr::addr_of_mut!((*tss).ist)
            .cast::<u64>()
            .add(0)
            .write_unaligned(IST1.top());
        core::ptr::addr_of_mut!((*tss).ist)
            .cast::<u64>()
            .add(1)
            .write_unaligned(IST2.top());
        core::ptr::addr_of_mut!((*tss).ist)
            .cast::<u64>()
            .add(2)
            .write_unaligned(IST3.top());
        core::ptr::addr_of_mut!((*tss).iomap_base).write_unaligned(size_of::<Tss>() as u16);
    }

    let tss_base = tss as usize as u64;
    let tss_limit = size_of::<Tss>().saturating_sub(1) as u32;
    let gdt = GDT.0.get();
    // SAFETY: the GDT static is owned by the kernel and written before `lgdt`.
    unsafe {
        let table = &mut *gdt;
        if let Some(slot) = table.get_mut(0) {
            *slot = 0;
        }
        if let Some(slot) = table.get_mut(1) {
            *slot = KERNEL_CODE_ENTRY;
        }
        if let Some(slot) = table.get_mut(2) {
            *slot = KERNEL_DATA_ENTRY;
        }
        if let Some(slot) = table.get_mut(3) {
            *slot = USER_DATA_ENTRY;
        }
        if let Some(slot) = table.get_mut(4) {
            *slot = USER_CODE_ENTRY;
        }
        if let Some(slot) = table.get_mut(5) {
            *slot = entry(tss_base, tss_limit, 0x89, 0);
        }
        if let Some(slot) = table.get_mut(6) {
            *slot = tss_base >> 32;
        }
    }

    let pointer = DescriptorTablePointer {
        limit: size_of::<[u64; GDT_ENTRIES]>().saturating_sub(1) as u16,
        base: gdt as usize as u64,
    };
    // SAFETY: the pointer describes the initialized static GDT.
    unsafe {
        load_gdt(&pointer);
    }
}

/// Set the TSS `rsp0` slot used for ring-3 to ring-0 transitions.
pub fn set_rsp0(value: u64) {
    let tss = TSS.0.get();
    // SAFETY: the TSS is a kernel static; `rsp[0]` is the architectural RSP0 field.  Only the
    // user-entry CPU (CPU 0 today) calls this, which is why one shared TSS is sufficient until
    // the per-CPU TSS requirement in the module documentation is implemented.
    unsafe {
        core::ptr::addr_of_mut!((*tss).rsp)
            .cast::<u64>()
            .write_unaligned(value);
    }
}

unsafe fn load_gdt(pointer: &DescriptorTablePointer) {
    // SAFETY: `pointer` is a valid 10-byte descriptor-table pointer and the GDT memory is mapped.
    unsafe {
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) pointer,
            options(nostack, preserves_flags),
        );
        // Reload CS through a far return; DS/ES/SS/FS/GS use the kernel data selector. This also
        // serializes the GDT load before the TSS is installed.
        core::arch::asm!(
            "push 0x08",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ax, {tss}",
            "ltr ax",
            tss = const TSS_SELECTOR,
            out("rax") _,
            options(preserves_flags),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_selectors_match_sysret_arithmetic() {
        let star = STAR_USER_BASE as u64;
        assert_eq!(star.saturating_add(8) & 0xffff, USER_SS as u64);
        assert_eq!(star.saturating_add(16) & 0xffff, USER_CS as u64);
        assert_eq!(KERNEL_CS, 0x08);
        assert_eq!(KERNEL_SS, 0x10);
    }
}
