// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S44 C19 fixture: kernel `sigaction`, the `sp - 128` frame, sigcode return and `sigaltstack`.
//!
//! The fixture installs a `SA_SIGINFO` handler through the new kernel syscall, blocks and raises
//! `SIGUSR1`, then unblocks it while capturing the interrupted `rip`/`rsp`. The handler checks
//! `rdi/rsi/rdx`, the `ucontext_t` PC/SP, the frame address, and that its return address is the
//! page-aligned sigcode. It then returns through the sigcode and the interrupted label resumes.
//! A second scenario installs `SA_ONSTACK` and proves the frame lands on the alternate stack.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

use raven_abi::signal::MContext;
use raven_abi::syscall::{PROC_SIGQUEUE, SIGACTION, SIGALTSTACK, SIGPROCMASK};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const SIGUSR1: i32 = 10;
const SIGUSR2: i32 = 12;
const SA_SIGINFO: i32 = 4;
const SA_ONSTACK: i32 = 0x0800_0000;
const SIG_BLOCK: usize = 0;
const SIG_UNBLOCK: usize = 1;
const SS_ONSTACK: u32 = 1;
const ALT_SIZE: usize = 8192;

/// System V amd64 `struct sigaction` (152 bytes).
#[repr(C)]
struct SigAction {
    handler: u64,
    mask: [u64; 16],
    flags: i32,
    _pad: i32,
    restorer: u64,
}

/// System V amd64 `stack_t` (24 bytes).
#[repr(C)]
struct Stack {
    ss_sp: u64,
    ss_flags: i32,
    _pad: i32,
    ss_size: u64,
}

/// System V amd64 `siginfo_t` (128 bytes).
#[repr(C)]
struct SigInfo {
    signo: i32,
    errno: i32,
    code: i32,
    _pad: i32,
    fields: [u8; 112],
}

/// System V amd64 `ucontext_t` (936 bytes).
#[repr(C)]
struct UContext {
    flags: u64,
    link: u64,
    stack: Stack,
    mcontext: MContext,
    sigmask: [u64; 16],
    fpregs: [u64; 64],
}

const _: () = assert!(core::mem::size_of::<SigAction>() == 152);
const _: () = assert!(core::mem::size_of::<Stack>() == 24);
const _: () = assert!(core::mem::size_of::<SigInfo>() == 128);
const _: () = assert!(core::mem::size_of::<UContext>() == 936);

static HANDLED: AtomicU32 = AtomicU32::new(0);
static H_SIG: AtomicI32 = AtomicI32::new(0);
static H_SI: AtomicI32 = AtomicI32::new(0);
static H_PC: AtomicU64 = AtomicU64::new(0);
static H_SP: AtomicU64 = AtomicU64::new(0);
static H_FRAME: AtomicU64 = AtomicU64::new(0);
static H_RET: AtomicU64 = AtomicU64::new(0);
static ALT_COUNT: AtomicU32 = AtomicU32::new(0);
static ALT_FRAME: AtomicU64 = AtomicU64::new(0);
static ALT_STACK_FLAGS: AtomicU32 = AtomicU32::new(0);

#[repr(C, align(16))]
struct AlternateStack([u8; ALT_SIZE]);
// Writable `.bss`: the kernel writes the signal frame into the alternate stack.
static mut ALT_STACK: AlternateStack = AlternateStack([0; ALT_SIZE]);

// The kernel enters the handler with `rsp = fp` (8 modulo 16) and the sigcode return address at
// `[fp]`. Save the frame in a callee-saved register, pass it as the fourth SysV argument, run the
// Rust body, restore `rsp` and `ret` into the sigcode at `[fp]`.
core::arch::global_asm!(
    r#"
    .section .text
    .global __s44_action_entry
    .type __s44_action_entry, @function
__s44_action_entry:
    mov r12, rsp
    mov rcx, r12
    call s44_action_body
    mov rsp, r12
    ret
    .size __s44_action_entry, . - __s44_action_entry
"#
);

unsafe extern "C" {
    fn __s44_action_entry();
}

#[unsafe(no_mangle)]
extern "C" fn s44_action_body(
    sig: i32,
    info: *const SigInfo,
    context: *const UContext,
    frame: u64,
) {
    let info = unsafe { &*info };
    let context = unsafe { &*context };
    if sig == SIGUSR2 {
        ALT_COUNT.fetch_add(1, Ordering::SeqCst);
        ALT_FRAME.store(frame, Ordering::SeqCst);
        ALT_STACK_FLAGS.store(context.stack.ss_flags as u32, Ordering::SeqCst);
        return;
    }
    HANDLED.fetch_add(1, Ordering::SeqCst);
    H_SIG.store(sig, Ordering::SeqCst);
    H_SI.store(info.signo, Ordering::SeqCst);
    H_PC.store(context.mcontext.pc(), Ordering::SeqCst);
    H_SP.store(context.mcontext.sp(), Ordering::SeqCst);
    H_FRAME.store(frame, Ordering::SeqCst);
    H_RET.store(unsafe { *(frame as *const u64) }, Ordering::SeqCst);
}

#[inline(always)]
unsafe fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

fn sigaction(signo: i32, action: &SigAction) -> isize {
    unsafe {
        syscall3(
            SIGACTION,
            signo as usize,
            action as *const SigAction as usize,
            0,
        )
    }
}

fn sigprocmask(how: usize, set: &u64) -> isize {
    unsafe { syscall3(SIGPROCMASK, how, set as *const u64 as usize, 0) }
}

/// Unblock `bit` with an inline syscall and return the interrupted `(rip, rsp)` the kernel sees.
fn unblock_and_capture(bit: u64) -> (u64, u64) {
    let mut pc: u64;
    let mut sp: u64;
    // SAFETY: the syscall takes a valid u64 pointer; the label is the instruction after it.
    unsafe {
        asm!(
            "mov {sp}, rsp",
            "lea {pc}, [rip + 2f]",
            "syscall",
            "2:",
            in("rax") SIGPROCMASK,
            in("rdi") SIG_UNBLOCK,
            in("rsi") core::ptr::addr_of!(bit),
            in("rdx") 0usize,
            sp = out(reg) sp,
            pc = out(reg) pc,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    (pc, sp)
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("signal-action: start");
    let action = SigAction {
        handler: __s44_action_entry as *const () as u64,
        mask: [0; 16],
        flags: SA_SIGINFO,
        _pad: 0,
        restorer: 0,
    };
    if sigaction(SIGUSR1, &action) != 0 {
        println!("signal-action: sigaction failed");
        sys::thread_exit(1);
    }
    println!("signal-action: sigaction ok");

    let bit = 1u64 << (SIGUSR1 - 1);
    if sigprocmask(SIG_BLOCK, &bit) != 0 {
        println!("signal-action: block failed");
        sys::thread_exit(1);
    }
    if unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) } != 0 {
        println!("signal-action: sigqueue failed");
        sys::thread_exit(1);
    }
    if HANDLED.load(Ordering::SeqCst) != 0 {
        println!("signal-action: delivered while blocked");
        sys::thread_exit(1);
    }
    let (pc, sp) = unblock_and_capture(bit);
    if HANDLED.load(Ordering::SeqCst) != 1 {
        println!("signal-action: handler did not run");
        sys::thread_exit(1);
    }
    if H_SIG.load(Ordering::SeqCst) != SIGUSR1 || H_SI.load(Ordering::SeqCst) != SIGUSR1 {
        println!("signal-action: wrong signal arguments");
        sys::thread_exit(1);
    }
    if H_PC.load(Ordering::SeqCst) != pc || H_SP.load(Ordering::SeqCst) != sp {
        println!("signal-action: context mismatch");
        sys::thread_exit(1);
    }
    let frame = H_FRAME.load(Ordering::SeqCst);
    if frame & 0xf != 8 || frame + 1072 > sp - 128 {
        println!("signal-action: frame placement wrong");
        sys::thread_exit(1);
    }
    let ret = H_RET.load(Ordering::SeqCst);
    if ret & 0xfff != 0 {
        println!("signal-action: sigcode return not page aligned");
        sys::thread_exit(1);
    }
    println!("signal-action: frame ok");
    println!("signal-action: resume ok");

    let action = SigAction {
        handler: __s44_action_entry as *const () as u64,
        mask: [0; 16],
        flags: SA_SIGINFO | SA_ONSTACK,
        _pad: 0,
        restorer: 0,
    };
    if sigaction(SIGUSR2, &action) != 0 {
        println!("signal-action: alt sigaction failed");
        sys::thread_exit(1);
    }
    println!("signal-action: alt sigaction ok");
    let stack = Stack {
        ss_sp: core::ptr::addr_of_mut!(ALT_STACK) as u64,
        ss_flags: 0,
        _pad: 0,
        ss_size: ALT_SIZE as u64,
    };
    if unsafe { syscall3(SIGALTSTACK, &stack as *const Stack as usize, 0, 0) } != 0 {
        println!("signal-action: sigaltstack failed");
        sys::thread_exit(1);
    }
    println!("signal-action: altstack set");
    let alt_start = core::ptr::addr_of_mut!(ALT_STACK) as u64;
    if unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR2 as usize, 0) } != 0 {
        println!("signal-action: alt sigqueue failed");
        sys::thread_exit(1);
    }
    if ALT_COUNT.load(Ordering::SeqCst) != 1 {
        println!("signal-action: alt handler did not run");
        sys::thread_exit(1);
    }
    let alt_frame = ALT_FRAME.load(Ordering::SeqCst);
    if alt_frame < alt_start || alt_frame + 1072 > alt_start + ALT_SIZE as u64 {
        println!("signal-action: frame not on altstack");
        sys::thread_exit(1);
    }
    if ALT_STACK_FLAGS.load(Ordering::SeqCst) & SS_ONSTACK == 0 {
        println!("signal-action: SS_ONSTACK missing");
        sys::thread_exit(1);
    }
    println!("signal-action: altstack ok");
    println!("signal-action: done");
    sys::thread_exit(0)
}
