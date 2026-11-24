// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Global descriptor table, per-CPU TSS and IST stacks.
//!
//! The GDT selector layout is fixed so the `STAR` arithmetic used by `sysret` in S08 is stable:
//! `STAR[63:48] = 0x13` yields `SS = 0x1B`, `CS = 0x23`. Every CPU uses the same selectors but
//! its own TSS, because `rsp0` and the IST stack pointers are architectural per-CPU state.
//!
//! # Per-CPU TSS
//!
//! Before S75 the kernel had one TSS shared by every CPU and `set_rsp0` was only called on the
//! ring-3 entry path pinned to CPU 0.  S75 gives every CPU its own TSS (and its own three IST
//! stacks) and its own GDT copy whose TSS descriptor points at that CPU's TSS.  The BSP loads
//! its GDT from [`init`]; each application processor loads its own with [`load`] during bring-up.

use core::cell::UnsafeCell;
use core::mem::{MaybeUninit, size_of};

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
/// Maximum CPUs in the per-CPU TSS/GDT tables; must equal `sched::smp::MAX_CPUS`.
const MAX_CPUS: usize = 64;

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

#[repr(C, align(16))]
struct Stack(UnsafeCell<[u8; IST_SIZE]>);

// SAFETY: the CPU writes the stack while delivering an exception; no Rust code holds a reference
// into it at the same time.
unsafe impl Sync for Stack {}

impl Stack {
    fn top(&self) -> u64 {
        (self as *const Self as u64).saturating_add(IST_SIZE as u64)
    }
}

struct TssCell(UnsafeCell<Tss>);
struct GdtCell(UnsafeCell<[u64; GDT_ENTRIES]>);

// SAFETY: a CPU writes its own TSS `rsp0` slot (one writer per CPU) and reads the IST pointers it
// set during init; the GDT entries are written once during init and then only read by the CPU.
unsafe impl Sync for TssCell {}
// SAFETY: the per-CPU GDT table is written once by the BSP during `init`, before any AP reads it.
unsafe impl Sync for GdtCell {}

struct TssTable(UnsafeCell<MaybeUninit<[TssCell; MAX_CPUS]>>);
struct GdtTable(UnsafeCell<MaybeUninit<[GdtCell; MAX_CPUS]>>);
struct IstTable(UnsafeCell<MaybeUninit<[Stack; MAX_CPUS]>>);

// SAFETY: each table is written once by the BSP during `init`, before any AP exists; afterwards
// it is only read, and the per-CPU TSS `rsp0` slot is written only by the owning CPU.
unsafe impl Sync for TssTable {}
// SAFETY: written once by the BSP before any AP reads it.
unsafe impl Sync for GdtTable {}
// SAFETY: the CPU writes its own IST stack during exception delivery; no Rust reference escapes.
unsafe impl Sync for IstTable {}

static TSS: TssTable = TssTable(UnsafeCell::new(MaybeUninit::uninit()));
static GDT: GdtTable = GdtTable(UnsafeCell::new(MaybeUninit::uninit()));
#[unsafe(link_section = ".bss.__ist")]
static IST1: IstTable = IstTable(UnsafeCell::new(MaybeUninit::uninit()));
#[unsafe(link_section = ".bss.__ist")]
static IST2: IstTable = IstTable(UnsafeCell::new(MaybeUninit::uninit()));
#[unsafe(link_section = ".bss.__ist")]
static IST3: IstTable = IstTable(UnsafeCell::new(MaybeUninit::uninit()));

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

fn tss_table() -> &'static [TssCell; MAX_CPUS] {
    // SAFETY: `init` writes the array before any AP starts; the BSP calls `load(0)` inside `init`.
    unsafe { &*(*TSS.0.get()).as_ptr() }
}

fn gdt_table() -> &'static [GdtCell; MAX_CPUS] {
    // SAFETY: `init` writes the array before any AP reads it.
    unsafe { &*(*GDT.0.get()).as_ptr() }
}

fn ist1_table() -> &'static [Stack; MAX_CPUS] {
    // SAFETY: `init` writes the array once before any CPU reads an IST pointer.
    unsafe { &*(*IST1.0.get()).as_ptr() }
}

fn ist2_table() -> &'static [Stack; MAX_CPUS] {
    // SAFETY: as `ist1_table`.
    unsafe { &*(*IST2.0.get()).as_ptr() }
}

fn ist3_table() -> &'static [Stack; MAX_CPUS] {
    // SAFETY: as `ist1_table`.
    unsafe { &*(*IST3.0.get()).as_ptr() }
}

fn tss_cell(cpu: usize) -> &'static TssCell {
    let table = tss_table();
    table
        .get(cpu.min(MAX_CPUS.saturating_sub(1)))
        .unwrap_or(&table[0])
}

fn gdt_cell(cpu: usize) -> &'static GdtCell {
    let table = gdt_table();
    table
        .get(cpu.min(MAX_CPUS.saturating_sub(1)))
        .unwrap_or(&table[0])
}

fn ist_top(cpu: usize, table: &[Stack; MAX_CPUS]) -> u64 {
    table
        .get(cpu.min(MAX_CPUS.saturating_sub(1)))
        .map_or(0, Stack::top)
}

/// Build one CPU's GDT: the fixed code/data selectors plus a TSS descriptor for that CPU's TSS.
fn build_gdt(tss_base: u64) -> [u64; GDT_ENTRIES] {
    let tss_limit = size_of::<Tss>().saturating_sub(1) as u32;
    let mut table = [0u64; GDT_ENTRIES];
    table[1] = KERNEL_CODE_ENTRY;
    table[2] = KERNEL_DATA_ENTRY;
    table[3] = USER_DATA_ENTRY;
    table[4] = USER_CODE_ENTRY;
    table[5] = entry(tss_base, tss_limit, 0x89, 0);
    table[6] = tss_base >> 32;
    table
}

/// Initialise every CPU's TSS and GDT and load the boot processor's.
pub fn init() {
    // SAFETY: `init` runs once on the boot processor before any AP exists; it writes the four
    // uninitialised statics exactly once, in place. `TssCell`, `GdtCell` and `Stack` are all-zero
    // at init, so the arrays are zeroed directly. Building them element-by-element (or with
    // `core::array::from_fn`) would materialise 16 KiB `Stack` temporaries on the 64 KiB boot
    // stack and overflow it.
    unsafe {
        let tss = (*TSS.0.get()).as_mut_ptr() as *mut u8;
        let gdt = (*GDT.0.get()).as_mut_ptr() as *mut u8;
        let ist1 = (*IST1.0.get()).as_mut_ptr() as *mut u8;
        let ist2 = (*IST2.0.get()).as_mut_ptr() as *mut u8;
        let ist3 = (*IST3.0.get()).as_mut_ptr() as *mut u8;
        core::ptr::write_bytes(tss, 0, size_of::<TssCell>().saturating_mul(MAX_CPUS));
        core::ptr::write_bytes(gdt, 0, size_of::<GdtCell>().saturating_mul(MAX_CPUS));
        core::ptr::write_bytes(ist1, 0, size_of::<Stack>().saturating_mul(MAX_CPUS));
        core::ptr::write_bytes(ist2, 0, size_of::<Stack>().saturating_mul(MAX_CPUS));
        core::ptr::write_bytes(ist3, 0, size_of::<Stack>().saturating_mul(MAX_CPUS));
    }

    let ist1 = ist1_table();
    let ist2 = ist2_table();
    let ist3 = ist3_table();
    for cpu in 0..MAX_CPUS {
        let tss = tss_cell(cpu).0.get();
        // SAFETY: the TSS static is owned by the kernel and has not been published yet.
        unsafe {
            core::ptr::addr_of_mut!((*tss).ist)
                .cast::<u64>()
                .add(0)
                .write_unaligned(ist_top(cpu, ist1));
            core::ptr::addr_of_mut!((*tss).ist)
                .cast::<u64>()
                .add(1)
                .write_unaligned(ist_top(cpu, ist2));
            core::ptr::addr_of_mut!((*tss).ist)
                .cast::<u64>()
                .add(2)
                .write_unaligned(ist_top(cpu, ist3));
            core::ptr::addr_of_mut!((*tss).iomap_base).write_unaligned(size_of::<Tss>() as u16);
        }
        let table = build_gdt(tss as usize as u64);
        let gdt = gdt_cell(cpu).0.get();
        // SAFETY: the GDT static is owned by the kernel and written before `lgdt`.
        unsafe {
            core::ptr::copy_nonoverlapping(table.as_ptr(), gdt.cast::<u64>(), GDT_ENTRIES);
        }
    }
    load(0);
}

/// Load `cpu`'s GDT and TSS on that CPU.
///
/// The CPU sets the TSS descriptor's busy bit when `ltr` loads it. An AP that is restarted (a
/// SIPI retry after a late start) must load the same TSS again; rebuilding the descriptor from
/// the available type clears that busy bit, otherwise `ltr` raises #GP.
pub fn load(cpu: usize) {
    let tss = tss_cell(cpu).0.get();
    let table = build_gdt(tss as usize as u64);
    let gdt = gdt_cell(cpu).0.get();
    // SAFETY: the owning CPU writes its own GDT immediately before `lgdt`; no other CPU reads
    // this per-CPU table.
    unsafe {
        core::ptr::copy_nonoverlapping(table.as_ptr(), gdt.cast::<u64>(), GDT_ENTRIES);
    }
    let pointer = DescriptorTablePointer {
        limit: size_of::<[u64; GDT_ENTRIES]>().saturating_sub(1) as u16,
        base: gdt as usize as u64,
    };
    // SAFETY: the pointer describes the initialized per-CPU GDT; this runs on the owning CPU.
    unsafe {
        load_gdt(&pointer);
    }
}

/// Set the `rsp0` slot of `cpu`'s TSS used for ring-3 to ring-0 transitions.
pub fn set_rsp0(cpu: usize, value: u64) {
    let tss = tss_cell(cpu).0.get();
    // SAFETY: the TSS is a kernel static; `rsp[0]` is the architectural RSP0 field.  Only the
    // owning CPU calls this, so there is one writer per CPU.
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
        // Reload CS through a far return; DS/ES/SS use the kernel data selector. FS and GS are
        // deliberately left alone: their bases come from the per-CPU MSRs (and user TLS), so
        // reloading the selector would reset them to the GDT descriptor's null base and break
        // `this_cpu()`. This also serializes the GDT load before the TSS is installed.
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
            "mov ax, {tss}",
            "ltr ax",
            tss = const TSS_SELECTOR,
            out("rax") _,
            options(preserves_flags),
        );
    }
}

#[cfg(test)]
#[path = "gdt_ut.rs"]
mod tests;
