// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Inter-processor interrupts: reschedule, TLB shootdown and cross-CPU probes.
//!
//! The stubs mirror the timer stub: ten general-purpose registers are saved above the CPU frame,
//! the Rust handler receives the interrupted stack pointer, and `swapgs` is paired for ring-3
//! entries. Each handler sends its own LAPIC end-of-interrupt.

use crate::arch::x86_64::{ap_start, apic, idt};
use crate::sched::smp;

/// Reschedule IPI vector.
pub const RESCHEDULE_VECTOR: u8 = 0x30;
/// TLB-shootdown IPI vector.
pub const TLB_VECTOR: u8 = 0x31;
/// Cross-CPU probe IPI vector.
pub const CALL_VECTOR: u8 = 0x32;
/// Application-processor LAPIC timer vector. The boot processor keeps the S07 timer vector 0x20
/// so `time::ticks()` continues to advance at exactly `HZ`.
pub const AP_TIMER_VECTOR: u8 = 0x21;

core::arch::global_asm!(
    r#"
    .section .text
    .macro ipi_stub name, target
    .global \name
    .type \name, @function
\name:
    test byte ptr [rsp+8], 3
    jz 1f
    swapgs
1:
    cld
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push rbp
    mov rbp, rsp
    mov rdi, rbp
    and rsp, -16
    call \target
    mov rsp, rbp
    pop rbp
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    test byte ptr [rsp+8], 3
    jz 2f
    swapgs
2:
    iretq
    .size \name, . - \name
    .endm

    ipi_stub reschedule_isr, reschedule_interrupt
    ipi_stub tlb_isr, tlb_interrupt
    ipi_stub call_isr, call_interrupt
    ipi_stub ap_timer_isr, ap_timer_interrupt

    /* NMI entry. Unlike a maskable IPI, an NMI can land inside the syscall entry/exit
     * `swapgs` window, where CS is already kernel but GS is still the user base. Read the
     * privileged KERNEL_GS_BASE MSR: a kernel address there means the bases are not swapped
     * yet, so swap before any per-CPU access. r15 holds the swap flag across the handler. */
    .macro nmi_ipi_stub name, target
    .global \name
    .type \name, @function
\name:
    cld
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push rbp
    push r15
    mov rbp, rsp
    xor r15d, r15d
    mov ecx, 0xc0000102
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov rcx, rax
    shr rcx, 47
    cmp rcx, 0x1ffff
    jb 1f
    swapgs
    mov r15, 1
1:
    mov rdi, rbp
    and rsp, -16
    call \target
    mov rsp, rbp
    test r15, r15
    jz 2f
    swapgs
2:
    pop r15
    pop rbp
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    iretq
    .size \name, . - \name
    .endm

    nmi_ipi_stub nmi_tlb_isr, nmi_tlb_interrupt
"#
);

unsafe extern "C" {
    fn reschedule_isr();
    fn tlb_isr();
    fn nmi_tlb_isr();
    fn call_isr();
    fn ap_timer_isr();
}

/// Install the IPI gates in the shared IDT. Returns false when a vector is out of range.
pub fn install() -> bool {
    idt::set_handler(
        u16::from(RESCHEDULE_VECTOR),
        reschedule_isr as *const () as u64,
    ) && idt::set_handler(u16::from(TLB_VECTOR), tlb_isr as *const () as u64)
        && idt::set_handler(2, nmi_tlb_isr as *const () as u64)
        && idt::set_handler(u16::from(CALL_VECTOR), call_isr as *const () as u64)
        && idt::set_handler(u16::from(AP_TIMER_VECTOR), ap_timer_isr as *const () as u64)
}

/// Send a fixed IPI to `cpu`.
pub fn send(cpu: usize, vector: u8) {
    if cpu >= smp::online_cpus() || cpu == smp::percpu::this_cpu() {
        return;
    }
    let destination = ap_start::apic_id(cpu);
    apic::send_ipi(destination, vector);
}

/// Ask `cpu` to reschedule.
pub fn send_reschedule(cpu: usize) {
    send(cpu, RESCHEDULE_VECTOR);
}

/// Ask `cpu` to flush its TLB.
///
/// S159 delivers the shootdown as an NMI so the target takes it even while IF is clear in a
/// long EXIT, usercopy-fault or scheduler critical section. A fixed IPI waits for IF=1 and can
/// miss the 100 ms deadline; the donor NetBSD shootdown waits indefinitely with interrupts
/// enabled (`x86_tlb.c` `pmap_tlb_shootnow`), which is not acceptable as a kernel-wide hang.
pub fn send_tlb(cpu: usize) {
    if cpu >= smp::online_cpus() || cpu == smp::percpu::this_cpu() {
        return;
    }
    let destination = ap_start::apic_id(cpu);
    apic::send_nmi(destination);
}

/// Ask `cpu` to flush its TLB with a maskable fixed IPI.
///
/// S195 sends this alongside the NMI: the local APIC keeps the fixed delivery pending while the
/// target is in an NMI handler or has IF clear, so a lost or blocked NMI cannot lose the
/// invalidation. The handler is idempotent (it ignores a sequence already flushed).
pub fn send_tlb_ipi(cpu: usize) {
    send(cpu, TLB_VECTOR);
}

/// Run the probe handler on `cpu`.
pub fn send_call(cpu: usize) {
    send(cpu, CALL_VECTOR);
}

#[unsafe(no_mangle)]
extern "C" fn reschedule_interrupt(rsp: u64) {
    // The stub saves ten GPRs above the CPU frame: RIP is at `rsp + 80`, CS at `rsp + 88`.
    // SAFETY: `rsp` is the interrupted kernel stack pointer; the CPU frame is present.
    let cs = unsafe { core::ptr::read_volatile(rsp.saturating_add(88) as *const u64) };
    apic::eoi();
    smp::reschedule_ipi(cs & 3 == 3);
}

#[unsafe(no_mangle)]
extern "C" fn tlb_interrupt(_rsp: u64) {
    let _ = smp::shootdown::handle_ipi();
    apic::eoi();
}

/// NMI form of the TLB shootdown (S159).
///
/// NMI is not an in-service local-APIC interrupt, so this handler deliberately does not send an
/// EOI. The handler only touches the shootdown atomics and `invlpg`; it is safe in any kernel
/// context, including an IF=0 critical section. An NMI with no pending shootdown is reported
/// through the emergency console, preserving the previous generic NMI diagnostic.
#[unsafe(no_mangle)]
extern "C" fn nmi_tlb_interrupt(_rsp: u64) {
    let cpu = crate::sched::smp::percpu::this_cpu();
    let pending = smp::shootdown::pending_sequence(cpu);
    if !smp::shootdown::handle_ipi() && pending == 0 {
        crate::kprintln_emergency!("EXCEPTION: nmi with no pending shootdown");
    }
}

#[unsafe(no_mangle)]
extern "C" fn call_interrupt(_rsp: u64) {
    smp::call::handle();
    apic::eoi();
}

#[unsafe(no_mangle)]
extern "C" fn ap_timer_interrupt(rsp: u64) {
    // SAFETY: `rsp` is the interrupted kernel stack pointer; RIP is at `rsp + 80`, CS at +88.
    let cs = unsafe { core::ptr::read_volatile(rsp.saturating_add(88) as *const u64) };
    crate::time::on_ap_tick();
    crate::sched::tick();
    apic::eoi();
    crate::sched::timer_return(cs & 3 == 3);
}

const _: () = assert!(RESCHEDULE_VECTOR != TLB_VECTOR);
const _: () = assert!(TLB_VECTOR != CALL_VECTOR);
const _: () = assert!(CALL_VECTOR != RESCHEDULE_VECTOR);
const _: () = assert!(AP_TIMER_VECTOR != 0x20);
