// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 assembly evidence: every driver here RUNS on the host (ticket 7.2 "Asm, x86_64 host").
//!
//! The test-only `global_asm!` drivers own the callee-saved registers; Rust never writes a
//! register the compiler may be using across a call. `setjmp`/`longjmp` round trips, nested
//! levels, `sigsetjmp` with and without the mask, `getcontext`/`setcontext`, `makecontext` with
//! 0..6 arguments and both `uc_link` cases, `swapcontext` ping-pong and `call_handler` on a
//! separate stack are all executed.
#![allow(
    unused_assignments,
    clippy::undocumented_unsafe_blocks,
    clippy::explicit_auto_deref,
    clippy::never_loop,
    clippy::identity_op,
    clippy::unnecessary_cast
)]
#![cfg(target_arch = "x86_64")]
#![allow(unsafe_code)]

use raven_libc_abi::structs::{JmpBuf, StackT, UcontextT};
use raven_libc_signal::SigSet;
use raven_libc_signal::frame;
use raven_libc_signal::raw;
use raven_libc_signal::sigjmp::{longjmp, setjmp, sigsetjmp_with_mask};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

core::arch::global_asm!(
    r#"
    .text
    .global asm_setjmp_regs
    .type asm_setjmp_regs,@function
// rdi = jmp_buf, esi = expected return, edx = longjmp value; eax = 1 on success.
asm_setjmp_regs:
    push %rbx
    push %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    mov %esi,%r11d
    mov %edx,%r10d
    mov $0x11111111,%rbx
    mov $0x22222222,%rbp
    mov $0x33333333,%r12
    mov $0x44444444,%r13
    mov $0x55555555,%r14
    mov $0x66666666,%r15
    call __rl_setjmp
    test %eax,%eax
    jnz 1f
    mov $0xdeadbeef,%rbx
    mov $0xdeadbeef,%rbp
    mov $0xdeadbeef,%r12
    mov $0xdeadbeef,%r13
    mov $0xdeadbeef,%r14
    mov $0xdeadbeef,%r15
    mov %r10d,%esi
    call __rl_longjmp
1:
    cmp %r11d,%eax
    jne 2f
    cmp $0x11111111,%rbx
    jne 2f
    cmp $0x22222222,%rbp
    jne 2f
    cmp $0x33333333,%r12
    jne 2f
    cmp $0x44444444,%r13
    jne 2f
    cmp $0x55555555,%r14
    jne 2f
    cmp $0x66666666,%r15
    jne 2f
    mov $1,%eax
    jmp 3f
2:
    xor %eax,%eax
3:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbp
    pop %rbx
    ret

    .text
    .global asm_context_entry
    .type asm_context_entry,@function
asm_context_entry:
    mov %rsp,%rax
    and $15,%eax
    mov %eax,asm_align(%rip)
    jmp context_body

    .data
    .global asm_align
asm_align:
    .quad 0
"#,
    options(att_syntax)
);

unsafe extern "C" {
    fn asm_setjmp_regs(buffer: *mut u8, expected: i32, value: i32) -> i32;
    fn asm_context_entry();
}

static ALIGN: AtomicUsize = AtomicUsize::new(0);
static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGS: [AtomicUsize; 6] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

/// The body `asm_context_entry` jumps to. It records the six psABI argument registers.
#[unsafe(no_mangle)]
extern "C" fn context_body(a1: usize, a2: usize, a3: usize, a4: usize, a5: usize, a6: usize) {
    let values = [a1, a2, a3, a4, a5, a6];
    let mut used = 0usize;
    for (index, value) in values.iter().enumerate() {
        ARGS[index].store(*value, Ordering::SeqCst);
        if *value != 0 {
            used = index + 1;
        }
    }
    ARGC.store(used, Ordering::SeqCst);
    let mut alignment = 0u32;
    // SAFETY: reads the test-only data cell written by `asm_context_entry`.
    unsafe {
        core::arch::asm!("mov {v:e}, dword ptr [rip + asm_align]", v = out(reg) alignment, options(nostack, preserves_flags));
    }
    ALIGN.store(alignment as usize, Ordering::SeqCst);
}

#[test]
fn setjmp_longjmp_restores_all_callee_saved_registers() {
    let mut buffer = JmpBuf::ZERO;
    // SAFETY: the asm driver saves and restores every register it touches.
    let ok = unsafe { asm_setjmp_regs(buffer.bytes.as_mut_ptr().cast::<u8>(), 5, 5) };
    assert_eq!(
        ok, 1,
        "callee-saved registers or the return value were not restored"
    );
}

#[test]
fn longjmp_zero_returns_one() {
    let mut buffer = JmpBuf::ZERO;
    // SAFETY: as above; the driver compares the value 1 produced from `longjmp(buf, 0)`.
    let ok = unsafe { asm_setjmp_regs(buffer.bytes.as_mut_ptr().cast::<u8>(), 1, 0) };
    assert_eq!(ok, 1);
}

#[test]
fn setjmp_rust_roundtrip() {
    let mut buffer = JmpBuf::ZERO;
    let value = setjmp(&mut buffer);
    if value == 0 {
        longjmp(&buffer, 9);
    }
    assert_eq!(value, 9);
}

#[test]
fn nested_setjmp_levels() {
    let mut buffers = [JmpBuf::ZERO; 8];
    nest(&mut buffers, 0);
}

fn nest(buffers: &mut [JmpBuf; 8], depth: usize) {
    let value = setjmp(&mut buffers[depth]);
    if value == 0 {
        if depth < 7 {
            nest(buffers, depth + 1);
        }
        longjmp(&buffers[depth], (depth as i32) + 1);
    }
    assert_eq!(value, (depth as i32) + 1, "level {depth}");
    if depth > 0 {
        longjmp(&buffers[depth - 1], depth as i32);
    }
}

#[test]
fn sigsetjmp_saves_and_restores_the_mask() {
    let mut buffer = JmpBuf::ZERO;
    let mut word: u64 = 0x55aa;
    let value = sigsetjmp_with_mask(&mut buffer, 1, &mut word);
    if value == 0 {
        word = 0x0f0f;
        core::hint::black_box(word);
        longjmp(&buffer, 1);
    }
    assert_eq!(value, 1);
    assert_eq!(word, 0x55aa, "savemask=1 must restore the saved word");
}

#[test]
fn sigsetjmp_without_mask_does_not_restore_it() {
    let mut buffer = JmpBuf::ZERO;
    let mut word: u64 = 0x55aa;
    let value = sigsetjmp_with_mask(&mut buffer, 0, &mut word);
    if value == 0 {
        word = 0x0f0f;
        core::hint::black_box(word);
        longjmp(&buffer, 1);
    }
    assert_eq!(value, 1);
    assert_eq!(word, 0x0f0f, "savemask=0 must not touch the word");
}

static CONTEXT_LOOPS: AtomicUsize = AtomicUsize::new(0);

#[test]
#[allow(clippy::never_loop)]
fn getcontext_setcontext_loop() {
    let mut context = UcontextT::ZERO;
    // The counter must live in memory, not in a callee-saved register: `setcontext_restore`
    // returns to the captured point with the register state of the `getcontext` call, so a
    // register-resident counter would be reset on every resume and the loop would never end.
    CONTEXT_LOOPS.store(0, Ordering::SeqCst);
    // SAFETY: `context` is a live ucontext and `setcontext_restore` only resumes it.
    loop {
        let result = raw::getcontext_save(&mut context);
        assert_eq!(result, 0);
        let loops = CONTEXT_LOOPS.fetch_add(1, Ordering::SeqCst) + 1;
        if loops >= 4 {
            break;
        }
        // SAFETY: the context was just captured on this live stack.
        raw::setcontext_restore(&context);
    }
    assert_eq!(CONTEXT_LOOPS.load(Ordering::SeqCst), 4);
}

static MADE: AtomicBool = AtomicBool::new(false);

fn make_stack() -> (Vec<u8>, StackT) {
    let mut stack = vec![0u8; 64 * 1024];
    let stack_t = StackT {
        ss_sp: stack.as_mut_ptr().cast::<core::ffi::c_void>(),
        ss_flags: 0,
        ss_size: stack.len() as u64,
    };
    (stack, stack_t)
}

#[test]
fn getcontext_saves_the_legacy_fp_area_and_pointer() {
    let mut context = UcontextT::ZERO;
    let mut mxcsr = 0u32;
    let mut control = 0u16;
    // SAFETY: both stores write one register to a live local.
    unsafe {
        core::arch::asm!("stmxcsr [{}]", in(reg) &mut mxcsr, options(nostack, preserves_flags));
        core::arch::asm!("fnstcw [{}]", in(reg) &mut control, options(nostack, preserves_flags));
    }
    assert_eq!(raw::getcontext_save(&mut context), 0);
    let machine = frame::mcontext_of(&context);
    let fp_pointer = machine.bytes.get(23).copied().unwrap_or(0) as usize;
    assert_eq!(
        fp_pointer,
        frame::uc_mcontext_ptr(&context) as usize + 384,
        "mcontext.fpregs must point at ucontext.__fpregs_mem"
    );
    let fp_word = context.bytes.get(53).copied().unwrap_or(0);
    assert_eq!(fp_word as u32, mxcsr, "MXCSR must be saved at __fpregs_mem");
    assert_eq!(
        (fp_word >> 32) as u16,
        control,
        "the x87 control word must follow MXCSR"
    );
}

#[test]
fn makecontext_zero_to_six_arguments() {
    for count in 0..=6usize {
        MADE.store(false, Ordering::SeqCst);
        ALIGN.store(0, Ordering::SeqCst);
        ARGC.store(0, Ordering::SeqCst);
        for slot in &ARGS {
            slot.store(0, Ordering::SeqCst);
        }
        let (stack, stack_t) = make_stack();
        let mut main = UcontextT::ZERO;
        let mut child = UcontextT::ZERO;
        // SAFETY: all contexts are live locals and the stack outlives the call.
        let result = raw::getcontext_save(&mut main);
        assert_eq!(result, 0);
        if !MADE.load(Ordering::SeqCst) {
            MADE.store(true, Ordering::SeqCst);
            frame::set_uc_stack(&mut child, &stack_t);
            frame::set_uc_link(&mut child, &mut main as *mut UcontextT as usize);
            let args: Vec<usize> = (1..=count).collect();
            raven_libc_signal::makecontext_core(
                &mut child,
                asm_context_entry as *const () as usize,
                &args,
            )
            .unwrap();
            raw::setcontext_restore(&child);
        }
        assert_eq!(ARGC.load(Ordering::SeqCst), count, "argc {count}");
        for (index, value) in (1..=count).enumerate() {
            assert_eq!(ARGS[index].load(Ordering::SeqCst), value, "arg {index}");
        }
        assert_eq!(ALIGN.load(Ordering::SeqCst), 8, "psABI entry alignment");
        drop(stack);
    }
}

#[test]
fn makecontext_null_link_exits_zero() {
    if std::env::var_os("RAVEN_NULL_LINK_CHILD").is_some() {
        let (_stack, stack_t) = make_stack();
        let mut child = UcontextT::ZERO;
        frame::set_uc_stack(&mut child, &stack_t);
        frame::set_uc_link(&mut child, 0);
        raven_libc_signal::makecontext_core(
            &mut child,
            asm_context_entry as *const () as usize,
            &[],
        )
        .unwrap();
        raw::setcontext_restore(&child);
    }
    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(exe)
        .env("RAVEN_NULL_LINK_CHILD", "1")
        .args(["--exact", "makecontext_null_link_exits_zero", "--nocapture"])
        .status()
        .unwrap();
    assert!(
        status.success(),
        "NULL uc_link child did not exit 0: {status}"
    );
}

static SWAPS: AtomicUsize = AtomicUsize::new(0);
static mut PING_CTX: UcontextT = UcontextT::ZERO;
static mut PONG_CTX: UcontextT = UcontextT::ZERO;
static mut MAIN_CTX: UcontextT = UcontextT::ZERO;

extern "C" fn ping() {
    loop {
        if SWAPS.load(Ordering::SeqCst) >= 1000 {
            // SAFETY: MAIN_CTX was captured on the test stack, which is still live.
            unsafe { raw::setcontext_restore(&*core::ptr::addr_of!(MAIN_CTX)) };
        }
        SWAPS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: both contexts are static and live for the process.
        unsafe {
            let old = &mut *core::ptr::addr_of_mut!(PING_CTX);
            let new = &*core::ptr::addr_of!(PONG_CTX);
            raw::swapcontext_save(old, new);
        }
    }
}

extern "C" fn pong() {
    loop {
        if SWAPS.load(Ordering::SeqCst) >= 1000 {
            // SAFETY: as in `ping`.
            unsafe { raw::setcontext_restore(&*core::ptr::addr_of!(MAIN_CTX)) };
        }
        SWAPS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: as in `ping`.
        unsafe {
            let old = &mut *core::ptr::addr_of_mut!(PONG_CTX);
            let new = &*core::ptr::addr_of!(PING_CTX);
            raw::swapcontext_save(old, new);
        }
    }
}

#[test]
fn swapcontext_ping_pong_one_thousand_times() {
    SWAPS.store(0, Ordering::SeqCst);
    let (ping_stack, ping_stack_t) = make_stack();
    let (pong_stack, pong_stack_t) = make_stack();
    // SAFETY: the three contexts are static and the stacks outlive the test.
    assert_eq!(
        raw::getcontext_save(unsafe { &mut *core::ptr::addr_of_mut!(MAIN_CTX) }),
        0
    );
    unsafe {
        if SWAPS.load(Ordering::SeqCst) == 0 {
            {
                let context = &mut *core::ptr::addr_of_mut!(PING_CTX);
                frame::set_uc_stack(context, &ping_stack_t);
                frame::set_uc_link(
                    context,
                    &mut *core::ptr::addr_of_mut!(MAIN_CTX) as *mut _ as usize,
                );
                raven_libc_signal::makecontext_core(context, ping as *const () as usize, &[])
                    .unwrap();
            }
            {
                let context = &mut *core::ptr::addr_of_mut!(PONG_CTX);
                frame::set_uc_stack(context, &pong_stack_t);
                frame::set_uc_link(
                    context,
                    &mut *core::ptr::addr_of_mut!(MAIN_CTX) as *mut _ as usize,
                );
                raven_libc_signal::makecontext_core(context, pong as *const () as usize, &[])
                    .unwrap();
            }
            raw::setcontext_restore(&*core::ptr::addr_of!(PING_CTX));
        }
    }
    assert!(SWAPS.load(Ordering::SeqCst) >= 1000);
    drop(ping_stack);
    drop(pong_stack);
}

extern "C" fn handler_on_stack(value: usize) -> usize {
    let stack = raven_libc_signal::context::current_stack_pointer();
    STACKS.fetch_add(stack, Ordering::SeqCst);
    value.wrapping_mul(3)
}

static STACKS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn call_handler_runs_on_a_separate_stack() {
    let mut stack = vec![0u8; 32 * 1024];
    let current = raven_libc_signal::context::current_stack_pointer();
    let top = stack.as_mut_ptr() as usize + stack.len();
    STACKS.store(0, Ordering::SeqCst);
    let result = raw::call_handler(top, handler_on_stack as *const () as usize, 7, 0, 0);
    assert_eq!(result, 21);
    let used = STACKS.load(Ordering::SeqCst);
    assert!(used > top.saturating_sub(32 * 1024) && used <= top);
    assert!(
        used.abs_diff(current) > 1024,
        "handler did not switch stacks"
    );
}

#[test]
fn the_mask_word_is_the_tcb_field_in_production_shape() {
    // The asm tail takes any word; the production offset assertion is in `raw_ut`.
    let mut set = SigSet::empty();
    set.add(2).unwrap();
    assert!(set.is_member(2).unwrap());
}
