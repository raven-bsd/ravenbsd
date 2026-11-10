// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Application-processor startup: real-mode trampoline, per-CPU GS block, APIC helpers.
//!
//! The trampoline is copied to physical [`TRAMPOLINE_PHYS`] (below 1 MiB) and entered through an
//! INIT-SIPI-SIPI sequence with startup vector [`TRAMPOLINE_VECTOR`]. It is position-dependent:
//! every absolute address in it is the fixed physical address of the copy. The 16-bit code loads
//! a temporary GDT, enters 32-bit protected mode, enables PAE and long mode with a private page
//! table that identity-maps the first 2 MiB, then enters 64-bit mode. The 64-bit tail installs
//! the per-CPU GS base and the final kernel GDT and jumps to [`ap_entry`].
//!
//! The private page table copies the kernel PML4 entries, so the high-half kernel mappings are
//! available immediately. [`ap_entry`] switches to the kernel root before touching the heap.
//!
//! Only kernel threads run on application processors in S10: ring-3 entry needs a per-CPU TSS,
//! which the follow-up SMP work owns. The boot processor keeps the existing TSS path.

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};

use raven_rmm::MapError;

use crate::arch::x86_64::{acpi, apic, pic, timer};
use crate::kprintln;
use crate::memory::{frame, mmio, paging};
use crate::time;

/// Physical page holding the trampoline (startup vector 0x08).
pub const TRAMPOLINE_PHYS: u64 = 0x8000;
/// Real-mode startup vector; `CS = vector << 8`, `IP = 0`.
pub const TRAMPOLINE_VECTOR: u8 = 0x08;
/// Number of CPUs with a GS block (must match `sched::smp::MAX_CPUS`).
pub const MAX_CPUS: usize = 64;

const IA32_APIC_BASE: u32 = 0x1B;
const IA32_GS_BASE: u32 = 0xC000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

const X2_TPR: u32 = 0x808;
const X2_SIVR: u32 = 0x80F;
const X2_ICR: u32 = 0x830;
const X2_LVT_TIMER: u32 = 0x832;
const X2_LVT_LINT0: u32 = 0x835;
const X2_LVT_LINT1: u32 = 0x836;
const X2_LVT_ERROR: u32 = 0x837;
const X2_DIV_CONF: u32 = 0x83E;

const XAPIC_TPR: u32 = 0x80;
const XAPIC_SVR: u32 = 0xF0;
const XAPIC_ICR_LOW: u32 = 0x300;
const XAPIC_ICR_HIGH: u32 = 0x310;
const XAPIC_LVT_TIMER: u32 = 0x320;
const XAPIC_LVT_LINT0: u32 = 0x350;
const XAPIC_LVT_LINT1: u32 = 0x360;
const XAPIC_LVT_ERROR: u32 = 0x370;
const XAPIC_DIV_CONF: u32 = 0x3E0;

/// LVT mask bit (kept local so the trampoline module does not depend on apic internals).
const LVT_MASK: u32 = 1 << 16;

/// Per-CPU data reachable through `GS`.
///
/// The first four fields match `usermode::PerCpu` and the syscall entry stub (`gs:[8]` is the
/// kernel stack, `gs:[16]` the user stack scratch slot). The remaining fields extend the block.
#[repr(C, align(64))]
pub struct PerCpuBlock {
    /// Pointer to this block (offset 0).
    pub self_ptr: AtomicU64,
    /// Kernel stack top for the syscall stub (offset 8).
    pub kernel_rsp: AtomicU64,
    /// User stack scratch slot written by the syscall stub (offset 16).
    pub user_rsp_scratch: AtomicU64,
    /// Current thread pointer (offset 24).
    pub current_thread: AtomicU64,
    /// CPU index (offset 32).
    pub cpu_index: AtomicU64,
    /// Local APIC id (offset 40).
    pub apic_id: AtomicU64,
    /// Set when the AP has finished architecture setup (offset 48).
    pub online: AtomicU64,
    /// Reserved for later per-CPU data (offset 56).
    pub reserved: AtomicU64,
}

impl PerCpuBlock {
    const fn new() -> Self {
        Self {
            self_ptr: AtomicU64::new(0),
            kernel_rsp: AtomicU64::new(0),
            user_rsp_scratch: AtomicU64::new(0),
            current_thread: AtomicU64::new(0),
            cpu_index: AtomicU64::new(0),
            apic_id: AtomicU64::new(0),
            online: AtomicU64::new(0),
            reserved: AtomicU64::new(0),
        }
    }
}

const _: () = {
    assert!(size_of::<PerCpuBlock>() == 64);
    assert!(core::mem::offset_of!(PerCpuBlock, self_ptr) == 0);
    assert!(core::mem::offset_of!(PerCpuBlock, kernel_rsp) == 8);
    assert!(core::mem::offset_of!(PerCpuBlock, user_rsp_scratch) == 16);
    assert!(core::mem::offset_of!(PerCpuBlock, current_thread) == 24);
    assert!(core::mem::offset_of!(PerCpuBlock, cpu_index) == 32);
};

static BLOCKS: [PerCpuBlock; MAX_CPUS] = [const { PerCpuBlock::new() }; MAX_CPUS];
static GS_READY: AtomicU64 = AtomicU64::new(0);
static APIC_MMIO: AtomicU64 = AtomicU64::new(0);
static AP_TIMER_INITIAL: AtomicU32 = AtomicU32::new(0);
static AP_ROOT: AtomicU64 = AtomicU64::new(0);
static AP_TIMERS_GO: AtomicBool = AtomicBool::new(false);
static BSP_IDT_LIMIT: AtomicU16 = AtomicU16::new(0);
static BSP_IDT_BASE: AtomicU64 = AtomicU64::new(0);
static NULL_PAGE_FRAME: AtomicU64 = AtomicU64::new(0);

/// Final 64-bit GDT used by application processors. 0x08 is the kernel code selector, 0x10 the
/// kernel data selector, matching the boot processor's GDT. No TSS descriptor is loaded because
/// APs only run kernel threads.
static FINAL_GDT: [u64; 5] = [
    0,
    0x00af_9a00_0000_ffff,
    0x00cf_9200_0000_ffff,
    0x00cf_f200_0000_ffff,
    0x00af_fa00_0000_ffff,
];

/// One CPU's GS block.
pub fn block(cpu: usize) -> &'static PerCpuBlock {
    BLOCKS
        .get(cpu.min(MAX_CPUS.saturating_sub(1)))
        .unwrap_or(&BLOCKS[0])
}

/// Virtual address of one CPU's GS block.
pub fn percpu_address(cpu: usize) -> u64 {
    block(cpu) as *const PerCpuBlock as u64
}

/// Index of the CPU executing this code, or 0 before the BSP GS block is installed.
#[inline]
pub fn this_cpu() -> usize {
    if GS_READY.load(Ordering::Acquire) == 0 {
        return 0;
    }
    // The block's self pointer is validated against the static array, so a temporary GS base of
    // zero (the legacy `syscall::init` path) falls back to CPU 0 instead of an out-of-bounds slot.
    let self_ptr = read_gs_self();
    let base = core::ptr::addr_of!(BLOCKS) as u64;
    let stride = size_of::<PerCpuBlock>() as u64;
    let end = base.saturating_add(stride.saturating_mul(MAX_CPUS as u64));
    if self_ptr < base || self_ptr >= end {
        return 0;
    }
    usize::try_from(
        self_ptr
            .saturating_sub(base)
            .checked_div(stride)
            .unwrap_or(0),
    )
    .unwrap_or(0)
    .min(MAX_CPUS.saturating_sub(1))
}

/// Install the GS base pair for `cpu` and publish its self pointer.
pub fn install_gs(cpu: usize) {
    let address = percpu_address(cpu);
    let block = block(cpu);
    block.self_ptr.store(address, Ordering::Release);
    block
        .cpu_index
        .store(u64::try_from(cpu).unwrap_or(0), Ordering::Release);
    wrmsr(IA32_GS_BASE, address);
    wrmsr(IA32_KERNEL_GS_BASE, address);
    GS_READY.store(1, Ordering::Release);
}

/// Update the kernel stack pointer used by `syscall` entry on this CPU.
pub fn set_kernel_rsp(value: u64) {
    block(this_cpu()).kernel_rsp.store(value, Ordering::Release);
}

/// Record the current thread pointer on this CPU.
pub fn set_current_thread(value: u64) {
    block(this_cpu())
        .current_thread
        .store(value, Ordering::Release);
}

/// Local APIC id recorded for `cpu`.
pub fn apic_id(cpu: usize) -> u32 {
    block(cpu).apic_id.load(Ordering::Acquire) as u32
}

fn read_gs_self() -> u64 {
    let value: u64;
    // SAFETY: GS:[0] is the self pointer of the installed per-CPU block; before installation the
    // base is zero and the real-mode IVT at linear 0 is readable. The value is validated by the
    // caller before it is used as a pointer.
    unsafe {
        asm!(
            "mov {}, qword ptr gs:[0]",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

/// Read one byte through the current address space. Used by the TLB self-test.
///
/// # Safety
///
/// `address` must be mapped readable in the current address space. The self-test maps its own
/// pages before calling this.
pub unsafe fn probe_read(address: u64) -> u8 {
    // SAFETY: the caller guarantees the mapping.
    unsafe { core::ptr::read_volatile(address as *const u8) }
}

/// Enable maskable interrupts on this CPU (used while waiting for a remote TLB acknowledgement).
pub fn enable_local_interrupts() {
    // SAFETY: the caller is in kernel context and all IDT gates are installed.
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

/// Disable maskable interrupts on this CPU.
pub fn disable_local_interrupts() {
    // SAFETY: `cli` is ring 0 and only clears IF.
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// Invalidate the local TLB for `len` bytes starting at `addr`.
pub fn flush_range_local(addr: u64, len: u64) {
    if len == 0 {
        return;
    }
    // A huge range is cheaper to flush as a whole. CR3 reload keeps global kernel entries; the
    // shootdown range is always a user or test mapping, so non-global entries cover it.
    if len > 64 * 4096 {
        flush_all_local();
        return;
    }
    let end = addr.saturating_add(len);
    let mut page = addr & !4095;
    while page < end {
        invlpg(page);
        page = page.saturating_add(4096);
    }
}

fn flush_all_local() {
    // SAFETY: reloading CR3 with its current value invalidates non-global TLB entries; it does
    // not change the address space.
    unsafe {
        asm!(
            "mov rax, cr3",
            "mov cr3, rax",
            out("rax") _,
            options(nostack, preserves_flags),
        );
    }
}

fn invlpg(address: u64) {
    // SAFETY: `invlpg` is ring 0; `address` is a canonical virtual address supplied by the
    // shootdown caller (validated user range or the kernel test range).
    unsafe {
        asm!(
            "invlpg [{}]",
            in(reg) address,
            options(nostack, preserves_flags),
        );
    }
}

/// Initialise the BSP GS block, map the APIC page, recalibrate the timer and build the AP root.
///
/// Called from the interrupt-enable hook with interrupts still disabled.
pub fn early_init() -> Result<(), ()> {
    install_gs(0);
    capture_bsp_idt();
    // `syscall::init` temporarily sets GS_BASE to zero before the scheduler repairs it. Mapping
    // virtual page 0 in the kernel root makes the `gs:[0]` self-pointer read return zero instead
    // of faulting if a timer interrupt lands in that window.
    map_null_page();
    block(0)
        .apic_id
        .store(u64::from(apic::id()), Ordering::Release);
    map_apic().map_err(|_| ())?;
    let initial = calibrate_timer();
    AP_TIMER_INITIAL.store(initial, Ordering::Release);
    let root = build_ap_root().ok_or(())?;
    AP_ROOT.store(root, Ordering::Release);
    Ok(())
}

/// Recalibrate the BSP LAPIC timer and return the periodic initial count.
///
/// `timer::init` already calibrated once, but it does not publish the count. Stopping the timer
/// here is safe: the scheduler has not started and interrupts are still disabled.
fn calibrate_timer() -> u32 {
    apic::timer_stop();
    let divisor = raven_timemath::lapic_divisor_bits(16).unwrap_or(0);
    apic::timer_set_divisor(divisor);
    apic::timer_set_lvt(LVT_MASK);
    apic::timer_set_initial(u32::MAX);
    let measured = pic::pit_wait_10ms();
    let current = apic::timer_current();
    apic::timer_stop();

    let initial = if measured {
        let counts = u64::from(u32::MAX.saturating_sub(current));
        raven_timemath::counts_per_second(counts, pic::pit_window_ns())
            .and_then(|rate| raven_timemath::lapic_initial_count(rate, timer::HZ))
            .filter(|value| *value != 0)
    } else {
        None
    };
    let initial = match initial {
        Some(initial) => initial,
        None => {
            kprintln!("smp: LAPIC timer recalibration failed");
            1_000_000
        }
    };
    apic::timer_start_periodic(timer::TIMER_VECTOR, initial);
    initial
}

/// Map the local APIC page for the raw xAPIC/IPI accesses.
fn map_apic() -> Result<(), MapError> {
    if apic::is_x2apic() {
        return Ok(());
    }
    let address = acpi::info().local_apic_address;
    let pointer = mmio::map_mmio(address, 0x1000)?;
    APIC_MMIO.store(pointer as u64, Ordering::Release);
    Ok(())
}

fn map_null_page() {
    let Some(frame) = frame::alloc_zeroed_frame() else {
        return;
    };
    let Some(virt) = raven_rmm::VirtAddr::new(0) else {
        return;
    };
    let mapped = paging::with_kernel_mapper(|mapper| {
        mapper.map(
            virt,
            raven_rmm::PhysAddr(frame.0),
            raven_rmm::PageFlags::kernel_rw(),
            raven_rmm::PageSize::Size4K,
        )
    });
    if matches!(mapped, Ok(Ok(()))) {
        NULL_PAGE_FRAME.store(frame.0, Ordering::Release);
    }
}

/// Remove the temporary null-page compatibility mapping once the GS base pair is repaired.
pub fn remove_null_page() {
    let Some(virt) = raven_rmm::VirtAddr::new(0) else {
        return;
    };
    let expected = NULL_PAGE_FRAME.load(Ordering::Acquire);
    let unmapped = paging::with_kernel_mapper(|mapper| mapper.unmap(virt));
    if let Ok(Ok(physical)) = unmapped
        && expected != 0
        && physical.0 == expected
    {
        let _ = frame::free_frame(frame::PhysAddr(physical.0));
        NULL_PAGE_FRAME.store(0, Ordering::Release);
    }
}

/// Build the private PML4 used between long-mode entry and [`ap_entry`].
///
/// The kernel PML4 entries are copied so the high half is available; entry 0 gets a 2 MiB
/// identity page covering the trampoline, the real-mode stack and low firmware memory.
fn build_ap_root() -> Option<u64> {
    let kernel = paging::kernel_root()?;
    let pml4 = frame::alloc_zeroed_frame()?;
    let pdpt = frame::alloc_zeroed_frame()?;
    let pd = frame::alloc_zeroed_frame()?;
    if pml4.0 >= 1 << 32 || pdpt.0 >= 1 << 32 || pd.0 >= 1 << 32 {
        return None;
    }
    let source = frame::phys_to_virt(frame::PhysAddr(kernel.0));
    let destination = frame::phys_to_virt(pml4);
    if source.is_null() || destination.is_null() {
        return None;
    }
    // SAFETY: both pointers are 4 KiB page-table frames reached through the HHDM; the copy is
    // exactly one PML4.
    unsafe {
        core::ptr::copy_nonoverlapping(source.cast::<u64>(), destination.cast::<u64>(), 512);
    }
    let pdpt_ptr = frame::phys_to_virt(pdpt);
    let pd_ptr = frame::phys_to_virt(pd);
    if pdpt_ptr.is_null() || pd_ptr.is_null() {
        return None;
    }
    // SAFETY: the freshly allocated frames are zeroed and writable through the HHDM; the writes
    // build present/rw tables and a 2 MiB identity leaf.
    unsafe {
        destination.cast::<u64>().write_volatile(pdpt.0 | 0x3);
        pdpt_ptr.cast::<u64>().write_volatile(pd.0 | 0x3);
        pd_ptr.cast::<u64>().write_volatile(0x83);
    }
    serialize();
    Some(pml4.0)
}

/// Allow the APs to start their local timers (called after the IPI gates are installed).
pub fn start_ap_timers() {
    AP_TIMERS_GO.store(true, Ordering::Release);
}

/// Private root built by [`early_init`].
pub fn ap_root() -> Result<u64, ()> {
    let root = AP_ROOT.load(Ordering::Acquire);
    if root == 0 { Err(()) } else { Ok(root) }
}

/// Copy the trampoline to low memory and patch one AP's startup fields.
pub fn prepare_ap(
    cpu: usize,
    apic_id: u32,
    stack_top: u64,
    entry: u64,
    pml4: u64,
) -> Result<(), ()> {
    if cpu == 0 || cpu >= MAX_CPUS {
        return Err(());
    }
    block(cpu)
        .apic_id
        .store(u64::from(apic_id), Ordering::Release);
    block(cpu).online.store(0, Ordering::Release);

    let destination = frame::phys_to_virt(frame::PhysAddr(TRAMPOLINE_PHYS));
    if destination.is_null() {
        return Err(());
    }
    let length = trampoline_length();
    if length != 4096 {
        return Err(());
    }
    // SAFETY: the destination is the reserved low page at TRAMPOLINE_PHYS mapped through the
    // HHDM; the source is the read-only trampoline blob in the kernel image of exactly `length`
    // bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(trampoline_start(), destination, length);
    }
    write_u64(destination, 0x180, pml4);
    write_u64(destination, 0x188, stack_top);
    write_u64(destination, 0x190, entry);
    write_u64(destination, 0x198, percpu_address(cpu));
    write_u64(destination, 0x1A0, u64::try_from(cpu).unwrap_or(0));
    write_u64(destination, 0x1A8, u64::from(apic_id));
    write_u16(
        destination,
        0x1F0,
        (FINAL_GDT.len() as u16).saturating_mul(8).saturating_sub(1),
    );
    write_u64(destination, 0x1F2, FINAL_GDT.as_ptr() as u64);
    serialize();
    Ok(())
}

fn trampoline_start() -> *const u8 {
    core::ptr::addr_of!(ap_trampoline_start)
}

fn trampoline_length() -> usize {
    let start = trampoline_start() as usize;
    let end = core::ptr::addr_of!(ap_trampoline_end) as usize;
    end.saturating_sub(start)
}

fn write_u16(base: *mut u8, offset: usize, value: u16) {
    // SAFETY: `offset` is inside the reserved trampoline page and `base` is writable.
    unsafe {
        base.add(offset).cast::<u16>().write_unaligned(value);
    }
}

fn write_u64(base: *mut u8, offset: usize, value: u64) {
    // SAFETY: `offset` is inside the reserved trampoline page and `base` is writable.
    unsafe {
        base.add(offset).cast::<u64>().write_unaligned(value);
    }
}

/// Serialise instruction and data streams after patching the trampoline.
fn serialize() {
    // SAFETY: `mfence`/`lfence` are ring 0 and CPUID is unprivileged; no memory is accessed.
    unsafe {
        asm!(
            "mfence",
            "lfence",
            "push rbx",
            "cpuid",
            "pop rbx",
            out("eax") _,
            out("ecx") _,
            out("edx") _,
            options(preserves_flags),
        );
    }
}

/// Send INIT, wait, send SIPI twice to `apic_id`.
pub fn start_ap(apic_id: u32) {
    const INIT_ASSERT: u32 = (0b101 << 8) | (1 << 14) | (1 << 15);
    const INIT_DEASSERT: u32 = (0b101 << 8) | (1 << 15);
    const SIPI: u32 = (0b110 << 8) | (TRAMPOLINE_VECTOR as u32) | (1 << 14) | (1 << 15);
    send_icr(apic_id, INIT_ASSERT);
    time::delay_ns(10_000_000);
    send_icr(apic_id, INIT_DEASSERT);
    time::delay_ns(200_000);
    send_icr(apic_id, SIPI);
    time::delay_ns(200_000);
    send_icr(apic_id, SIPI);
}

fn send_icr(destination: u32, value: u32) {
    if apic::is_x2apic() {
        wrmsr(X2_ICR, (u64::from(destination) << 32) | u64::from(value));
        return;
    }
    let base = APIC_MMIO.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    let pointer = base as usize;
    wait_delivery(pointer);
    mmio_write(pointer, XAPIC_ICR_HIGH, (destination & 0xFF) << 24);
    mmio_write(pointer, XAPIC_ICR_LOW, value);
    wait_delivery(pointer);
}

fn wait_delivery(base: usize) {
    let mut spins = 0u32;
    while mmio_read(base, XAPIC_ICR_LOW) & (1 << 12) != 0 {
        spins = spins.saturating_add(1);
        if spins >= 1_000_000 {
            return;
        }
        core::hint::spin_loop();
    }
}

fn mmio_read(base: usize, offset: u32) -> u32 {
    // SAFETY: `base` is the mapped APIC page and `offset` is an architectural register inside it.
    unsafe { core::ptr::read_volatile(base.saturating_add(offset as usize) as *const u32) }
}

fn mmio_write(base: usize, offset: u32, value: u32) {
    // SAFETY: `base` is the mapped APIC page and `offset` is an architectural register inside it.
    unsafe {
        core::ptr::write_volatile(base.saturating_add(offset as usize) as *mut u32, value);
    }
}

/// Read the boot processor's IDTR before any AP starts.
fn capture_bsp_idt() {
    #[repr(C, packed)]
    struct Descriptor {
        limit: u16,
        base: u64,
    }
    let mut descriptor = Descriptor { limit: 0, base: 0 };
    // SAFETY: `sidt` writes the 10-byte IDTR image into the local descriptor.
    unsafe {
        asm!(
            "sidt [{}]",
            in(reg) core::ptr::addr_of_mut!(descriptor),
            options(nostack, preserves_flags),
        );
    }
    BSP_IDT_LIMIT.store(descriptor.limit, Ordering::Release);
    BSP_IDT_BASE.store(descriptor.base, Ordering::Release);
}

/// Load the boot processor's IDT on this AP.
fn load_bsp_idt() {
    #[repr(C, packed)]
    struct Descriptor {
        limit: u16,
        base: u64,
    }
    let descriptor = Descriptor {
        limit: BSP_IDT_LIMIT.load(Ordering::Acquire),
        base: BSP_IDT_BASE.load(Ordering::Acquire),
    };
    // SAFETY: the base and limit were captured from the boot processor's `lidt`-valid table and
    // point at the shared kernel IDT.
    unsafe {
        asm!(
            "lidt [{}]",
            in(reg) core::ptr::addr_of!(descriptor),
            options(nostack, preserves_flags),
        );
    }
}

/// Enable this AP's local APIC and mask its legacy LINT inputs.
fn lapic_init_ap() {
    let mut base = rdmsr(IA32_APIC_BASE) | (1 << 11);
    if apic::is_x2apic() {
        base |= 1 << 10;
    } else {
        base &= !(1 << 10);
    }
    wrmsr(IA32_APIC_BASE, base);

    let divisor = u32::from(raven_timemath::lapic_divisor_bits(16).unwrap_or(0)) & 0xF;
    if apic::is_x2apic() {
        wrmsr(X2_TPR, 0);
        wrmsr(X2_SIVR, 0x1FF);
        wrmsr(X2_LVT_LINT0, u64::from(LVT_MASK));
        wrmsr(X2_LVT_LINT1, u64::from(LVT_MASK));
        wrmsr(X2_LVT_ERROR, u64::from(LVT_MASK));
        wrmsr(X2_LVT_TIMER, u64::from(LVT_MASK));
        wrmsr(X2_DIV_CONF, u64::from(divisor));
    } else {
        let base = APIC_MMIO.load(Ordering::Acquire);
        if base == 0 {
            return;
        }
        let pointer = base as usize;
        mmio_write(pointer, XAPIC_TPR, 0);
        mmio_write(pointer, XAPIC_SVR, 0x1FF);
        mmio_write(pointer, XAPIC_LVT_LINT0, LVT_MASK);
        mmio_write(pointer, XAPIC_LVT_LINT1, LVT_MASK);
        mmio_write(pointer, XAPIC_LVT_ERROR, LVT_MASK);
        mmio_write(pointer, XAPIC_LVT_TIMER, LVT_MASK);
        mmio_write(pointer, XAPIC_DIV_CONF, divisor);
    }
}

/// Rust entry point of an application processor.
///
/// Reached from the 64-bit trampoline tail with the AP kernel stack, final GDT and GS base
/// already installed and interrupts disabled.
#[unsafe(no_mangle)]
pub extern "C" fn ap_entry(cpu: u64) -> ! {
    let cpu = usize::try_from(cpu)
        .unwrap_or(0)
        .min(MAX_CPUS.saturating_sub(1));
    if let Some(root) = paging::kernel_root() {
        write_cr3(root.0);
    }
    install_gs(cpu);
    // Load the boot processor's IDT base. The table content is shared and the BSP has already
    // installed the IPI and AP-timer gates, so this AP sees them without rebuilding the table.
    load_bsp_idt();
    lapic_init_ap();
    block(cpu).online.store(1, Ordering::Release);
    // Wait until the BSP has installed the IPI gates before enabling this AP's timer.
    while !AP_TIMERS_GO.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    let initial = AP_TIMER_INITIAL.load(Ordering::Acquire);
    if initial != 0 {
        apic::timer_start_periodic(crate::arch::x86_64::ipi::AP_TIMER_VECTOR, initial);
    }
    crate::sched::smp::ap_main(cpu)
}

fn write_cr3(value: u64) {
    // SAFETY: `value` is the kernel page-table root built by `paging::init`.
    unsafe {
        asm!("mov cr3, {}", in(reg) value, options(nostack, preserves_flags));
    }
}

fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: ring-0 MSR read of an architectural address.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}

fn wrmsr(msr: u32, value: u64) {
    // SAFETY: ring-0 MSR write of an architectural address.
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags),
        );
    }
}

unsafe extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
}

core::arch::global_asm!(
    r#"
    .section .rodata.ap_trampoline, "a", @progbits
    .code16
    .global ap_trampoline_start
    .type ap_trampoline_start, @function
ap_trampoline_start:
    cli
    cld
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    lgdt [0x81e0]

    mov eax, cr0
    or eax, 1
    mov cr0, eax
    .byte 0x66, 0xea
    .long 0x8200
    .word 0x08

    .org 0x100
    /* Temporary GDT: 0x08 = 32-bit code, 0x10 = data, 0x18 = 64-bit code, 0x20 = data. */
    .quad 0
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff

    .org 0x180
    /* Patched by prepare_ap. */
    .quad 0                     /* pml4 physical */
    .quad 0                     /* stack top */
    .quad 0                     /* Rust entry */
    .quad 0                     /* GS block */
    .quad 0                     /* CPU index */
    .quad 0                     /* APIC id */

    .org 0x1e0
    .word 0x0027                /* temporary GDT limit (5 entries) */
    .long 0x8100                /* temporary GDT physical base */

    .org 0x1f0
    .word 0x0027                /* final GDT limit (patched) */
    .quad 0                     /* final GDT base (patched) */

    .org 0x200
    .code32
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax
    mov esp, 0x7000

    mov eax, cr4
    or eax, 0x20                /* PAE */
    mov cr4, eax

    mov eax, dword ptr [0x8180]
    mov cr3, eax

    mov ecx, 0xc0000080
    rdmsr
    or eax, 0x900               /* LME | NXE */
    wrmsr

    mov eax, cr0
    or eax, 0x80010001          /* PG | WP | PE */
    mov cr0, eax
    .byte 0xea
    .long 0x8300
    .word 0x18

    .org 0x300
    .code64
    mov rsp, qword ptr [0x8188]
    sub rsp, 8
    mov rax, qword ptr [0x8198]
    mov ecx, 0xc0000101
    mov rdx, rax
    shr rdx, 32
    wrmsr
    mov ecx, 0xc0000102
    wrmsr
    mov rdi, qword ptr [0x81a0]
    lgdt [0x81f0]
    mov rax, qword ptr [0x8190]
    push 0x08
    push rax
    retfq

    .org 0x1000
    .global ap_trampoline_end
ap_trampoline_end:
"#
);
