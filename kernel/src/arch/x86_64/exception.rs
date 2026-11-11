// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exception and interrupt stubs and reporting.

use crate::arch::x86_64::trapframe::TrapFrame;
use crate::{backtrace, kprintln};
use raven_abi::layout::x86_64::HHDM_BASE;

/// Direct-map window above [`HHDM_BASE`] valid for PC/return-address checks.
const HHDM_WINDOW: u64 = 1 << 47;

const NAMES: [&str; 32] = [
    "divide error",
    "debug",
    "nmi",
    "breakpoint",
    "overflow",
    "bound range",
    "invalid opcode",
    "device not available",
    "double fault",
    "coprocessor segment overrun",
    "invalid tss",
    "segment not present",
    "stack-segment fault",
    "general protection",
    "page fault",
    "reserved",
    "x87 floating-point",
    "alignment check",
    "machine check",
    "simd floating-point",
    "virtualization",
    "control protection",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
];

unsafe extern "C" {
    static ISR_TABLE: [u64; 256];
    static __kernel_start: u8;
    static __kernel_end: u8;
}

core::arch::global_asm!(
    r#"
    .section .text
    .global isr_common
    .type isr_common, @function
isr_common:
    /* Saved CS is at [rsp+24] for both error-code and dummy-error stubs. */
    test byte ptr [rsp+24], 3
    jz 1f
    swapgs
1:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rbp, rsp
    and rsp, -16
    mov rdi, rbp
    call exception_dispatch
    mov rsp, rbp
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16
    /* Saved CS is at [rsp+8] now; pair the entry swapgs for ring 3. */
    test byte ptr [rsp+8], 3
    jz 2f
    swapgs
2:
    iretq

    .macro isr n
    .global isr_\n
isr_\n:
    .if (\n == 8) || (\n == 10) || (\n == 11) || (\n == 12) || (\n == 13) || (\n == 14) || (\n == 17) || (\n == 21) || (\n == 29) || (\n == 30)
    push \n
    .else
    push 0
    push \n
    .endif
    jmp isr_common
    .endm

    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255
    isr \n
    .endr

    .section .rodata
    .global ISR_TABLE
ISR_TABLE:
    .irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255
    .quad isr_\n
    .endr
"#
);

/// Address of the stub for `vector`, or zero for an out-of-range vector.
pub fn isr_address(vector: u16) -> u64 {
    if vector as usize >= 256 {
        return 0;
    }
    // SAFETY: `vector` was bounds checked and the table has 256 entries.
    unsafe {
        core::ptr::addr_of!(ISR_TABLE)
            .cast::<u64>()
            .add(vector as usize)
            .read()
    }
}

/// Rust side of the exception stubs.
#[unsafe(no_mangle)]
pub extern "C" fn exception_dispatch(frame: &mut TrapFrame) {
    let vector = frame.vector as usize;
    let from_user = frame.cs & 3 == 3;
    if vector == 3 {
        kprintln!("EXCEPTION: breakpoint");
        return;
    }
    if vector == 14 {
        let cr2 = read_cr2();
        if crate::usercopy::handle_page_fault(frame, cr2) {
            return;
        }
        if from_user {
            if crate::process::handle_page_fault(cr2) {
                return;
            }
            crate::process::page_fault(cr2);
        }
    }
    if vector >= 32 {
        if crate::driver::irq::handle_interrupt(vector as u8) {
            return;
        }
        // An interrupt outside the driver pool.  Do not take the serial lock here: record the
        // vector and let the next IRQ syscall report it from process context.
        crate::driver::irq::note_unexpected(vector as u8);
        return;
    }
    if from_user {
        crate::process::user_exception(vector);
    }

    let name = NAMES.get(vector).copied().unwrap_or("unknown");
    kprintln!(
        "EXCEPTION: {} (vector {}, error {:#x})",
        name,
        vector,
        frame.error_code
    );
    kprintln!(
        "r15={:016x} r14={:016x} r13={:016x} r12={:016x} r11={:016x} r10={:016x}",
        frame.r15,
        frame.r14,
        frame.r13,
        frame.r12,
        frame.r11,
        frame.r10
    );
    kprintln!(
        "r9={:016x} r8={:016x} rbp={:016x} rdi={:016x} rsi={:016x} rdx={:016x}",
        frame.r9,
        frame.r8,
        frame.rbp,
        frame.rdi,
        frame.rsi,
        frame.rdx
    );
    kprintln!(
        "rcx={:016x} rbx={:016x} rax={:016x} vector={} error={:#x}",
        frame.rcx,
        frame.rbx,
        frame.rax,
        frame.vector,
        frame.error_code
    );
    kprintln!(
        "rip={:016x} cs={:016x} rflags={:016x} rsp={:016x} ss={:016x}",
        frame.rip,
        frame.cs,
        frame.rflags,
        frame.rsp,
        frame.ss
    );

    if vector == 14 {
        kprintln!("cr2=0x{:016x}", read_cr2());
        let error = frame.error_code;
        kprintln!(
            "page-fault: present={} write={} user={} reserved={} instruction={}",
            error & 1,
            (error >> 1) & 1,
            (error >> 2) & 1,
            (error >> 3) & 1,
            (error >> 4) & 1
        );
    }
    kprintln!("cr3=0x{:016x}", read_cr3());
    kprintln!("handler_rsp=0x{:016x}", frame as *const TrapFrame as u64);
    dump_backtrace(frame);

    if crate::log::test_exit() {
        crate::boot::exit_qemu(0x11);
    }
    crate::arch::halt();
}

fn read_cr2() -> u64 {
    let value: u64;
    // SAFETY: reading CR2 is a ring-0 operation with no memory side effects.
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn read_cr3() -> u64 {
    let value: u64;
    // SAFETY: reading CR3 is a ring-0 operation with no memory side effects.
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn valid_backtrace_address(address: u64) -> bool {
    let kernel_start = core::ptr::addr_of!(__kernel_start) as u64;
    let kernel_end = core::ptr::addr_of!(__kernel_end) as u64;
    let hhdm_end = HHDM_BASE.saturating_add(HHDM_WINDOW);
    (address >= kernel_start && address < kernel_end)
        || (address >= HHDM_BASE && address < hhdm_end)
}

fn dump_backtrace(frame: &TrapFrame) {
    let read = |address: u64| -> Option<u64> {
        let end = address.checked_add(8)?;
        if !valid_backtrace_address(address) || !valid_backtrace_address(end) {
            return None;
        }
        // SAFETY: both ends of the 8-byte read are inside the kernel image or HHDM window.
        unsafe { Some(core::ptr::read_volatile(address as *const u64)) }
    };
    kprintln!("backtrace:");
    let count = backtrace::walk(
        frame.rbp,
        read,
        valid_backtrace_address,
        |depth, rbp, rip| {
            kprintln!("bt[{}] rip=0x{:016x} rbp=0x{:016x}", depth, rip, rbp);
        },
    );
    if count == 0 {
        kprintln!("bt[] <no frame>");
    }
}
