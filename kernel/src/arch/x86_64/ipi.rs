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
"#
);

unsafe extern "C" {
    fn reschedule_isr();
    fn tlb_isr();
    fn call_isr();
    fn ap_timer_isr();
}

/// Install the IPI gates in the shared IDT. Returns false when a vector is out of range.
pub fn install() -> bool {
    idt::set_handler(
        u16::from(RESCHEDULE_VECTOR),
        reschedule_isr as *const () as u64,
    ) && idt::set_handler(u16::from(TLB_VECTOR), tlb_isr as *const () as u64)
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
pub fn send_tlb(cpu: usize) {
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
    smp::shootdown::handle_ipi();
    apic::eoi();
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
    crate::sched::tick();
    apic::eoi();
    crate::sched::timer_return(cs & 3 == 3);
}

const _: () = assert!(RESCHEDULE_VECTOR != TLB_VECTOR);
const _: () = assert!(TLB_VECTOR != CALL_VECTOR);
const _: () = assert!(CALL_VECTOR != RESCHEDULE_VECTOR);
const _: () = assert!(AP_TIMER_VECTOR != 0x20);
