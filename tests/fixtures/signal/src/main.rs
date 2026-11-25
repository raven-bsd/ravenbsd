// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S44/S161 C19 fixture: kernel `sigaction`, the kernel `sendsig` frame, sigcode return and
//! `sigaltstack`, on both supported architectures.
//!
//! The fixture installs a `SA_SIGINFO` handler through the new kernel syscall, blocks and raises
//! `SIGUSR1`, then unblocks it while capturing the interrupted pc/sp. The handler checks the
//! signal arguments, the `ucontext_t` pc/sp, the frame address, and that its return address is the
//! page-aligned sigcode. It then returns through the sigcode and the interrupted label resumes.
//! A second scenario installs `SA_ONSTACK` and proves the frame lands on the alternate stack.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
use raven_abi::signal::MContext;
use raven_abi::syscall::{PROC_SIGQUEUE, SIGACTION, SIGALTSTACK, SIGPROCMASK};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

mod guard;

entry!(main);

const SIGUSR1: i32 = 10;
const SIGUSR2: i32 = 12;
const SA_SIGINFO: u32 = 4;
const SA_ONSTACK: u32 = 0x0800_0000;
const SIG_BLOCK: usize = 0;
const SIG_UNBLOCK: usize = 1;
const SS_ONSTACK: u32 = 1;
const ALT_SIZE: usize = 8192;
const SIGFRAME_SIZE: usize = 0x410;
const SIGINFO_SIZE: usize = 0x50;

/// The Raven private 40-byte `SIGACTION` request (C19; `raven-libc-signal/src/sigaction_abi.rs`
/// and `kernel/src/syscall/thread.rs:UserSigAction`). This is not the public C `struct sigaction`.
#[repr(C)]
struct SigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

impl SigAction {
    const fn zero() -> Self {
        Self {
            handler: 0,
            mask: [0; 2],
            flags: 0,
            _pad: 0,
            restorer: 0,
        }
    }
}

/// Linux/System V `stack_t` (24 bytes) for `sigaltstack`.
#[repr(C)]
struct Stack {
    ss_sp: u64,
    ss_flags: i32,
    _pad: i32,
    ss_size: u64,
}

/// The C19 `siginfo_t` (S160 0x50): the fixture reads the signal number.
#[cfg(target_arch = "aarch64")]
#[repr(C)]
struct SigInfo {
    signo: i32,
    errno: i32,
    code: i32,
    _pad: i32,
    fields: [u8; SIGINFO_SIZE - 16],
}

/// System V amd64 `siginfo_t` (128 bytes).
#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct SigInfo {
    signo: i32,
    errno: i32,
    code: i32,
    _pad: i32,
    fields: [u8; 112],
}

/// aarch64 `mcontext_t` (NetBSD donor: `__gregs[35]`, `__fregs`, `__spare[8]`).
#[cfg(target_arch = "aarch64")]
#[repr(C, align(16))]
struct Aarch64Mcontext {
    gregs: [u64; 35],
    fregs: [u8; 0x210],
    spare: [u64; 8],
}

/// The C19/S160 aarch64 `stack_t`.
#[cfg(target_arch = "aarch64")]
#[repr(C)]
struct StackLayout {
    ss_sp: u64,
    ss_size: u64,
    ss_flags: u32,
    spare: u32,
}

/// C19 aarch64 `ucontext_t` (0x3c0).
#[cfg(target_arch = "aarch64")]
#[repr(C, align(16))]
struct UContext {
    sigmask: [u32; 4],
    mcontext: Aarch64Mcontext,
    link: u64,
    stack: StackLayout,
    flags: i32,
    spare: [i32; 4],
}

/// System V amd64 `ucontext_t` (936 bytes).
#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct UContext {
    flags: u64,
    link: u64,
    stack: Stack,
    mcontext: MContext,
    sigmask: [u64; 16],
    fpregs: [u64; 64],
}

const _: () = assert!(core::mem::size_of::<SigAction>() == 40);
const _: () = assert!(core::mem::offset_of!(SigAction, handler) == 0);
const _: () = assert!(core::mem::offset_of!(SigAction, mask) == 8);
const _: () = assert!(core::mem::offset_of!(SigAction, flags) == 24);
const _: () = assert!(core::mem::offset_of!(SigAction, restorer) == 32);
const _: () = assert!(core::mem::size_of::<Stack>() == 24);
#[cfg(target_arch = "x86_64")]
const _: () = assert!(core::mem::size_of::<SigInfo>() == 128);
#[cfg(target_arch = "x86_64")]
const _: () = assert!(core::mem::size_of::<UContext>() == 936);
#[cfg(target_arch = "aarch64")]
const _: () = assert!(core::mem::size_of::<SigInfo>() == SIGINFO_SIZE);
#[cfg(target_arch = "aarch64")]
const _: () = assert!(core::mem::size_of::<UContext>() == 0x3c0);

#[cfg(target_arch = "x86_64")]
impl UContext {
    fn pc(&self) -> u64 {
        self.mcontext.pc()
    }

    fn sp(&self) -> u64 {
        self.mcontext.sp()
    }

    fn stack_flags(&self) -> u32 {
        self.stack.ss_flags as u32
    }
}

#[cfg(target_arch = "aarch64")]
impl UContext {
    fn pc(&self) -> u64 {
        self.mcontext.gregs[32]
    }

    fn sp(&self) -> u64 {
        self.mcontext.gregs[31]
    }

    fn stack_flags(&self) -> u32 {
        self.stack.ss_flags
    }
}

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

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn __s44_action_entry();
}

#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
extern "C" fn s44_action_body(
    sig: i32,
    info: *const SigInfo,
    context: *const UContext,
    frame: u64,
) {
    let ret = unsafe { *(frame as *const u64) };
    action_body(sig, info, context, frame, ret);
}

/// On aarch64 the kernel enters the handler directly with `x0/x1/x2`, `x28 = &uc`, and
/// `x30 = sigcode`; the frame is `uc - SIGINFO_SIZE` and the return address is `x30`.
#[cfg(target_arch = "aarch64")]
#[unsafe(no_mangle)]
extern "C" fn s44_action_body(sig: i32, info: *const SigInfo, context: *const UContext) {
    let ret: u64;
    unsafe {
        asm!("mov {}, x30", out(reg) ret, options(nomem, nostack));
    }
    let frame = context as u64 - SIGINFO_SIZE as u64;
    action_body(sig, info, context, frame, ret);
}

fn action_body(sig: i32, info: *const SigInfo, context: *const UContext, frame: u64, ret: u64) {
    let info = unsafe { &*info };
    let context = unsafe { &*context };
    if sig == SIGUSR2 {
        ALT_COUNT.fetch_add(1, Ordering::SeqCst);
        ALT_FRAME.store(frame, Ordering::SeqCst);
        ALT_STACK_FLAGS.store(context.stack_flags(), Ordering::SeqCst);
        return;
    }
    HANDLED.fetch_add(1, Ordering::SeqCst);
    H_SIG.store(sig, Ordering::SeqCst);
    H_SI.store(info.signo, Ordering::SeqCst);
    H_PC.store(context.pc(), Ordering::SeqCst);
    H_SP.store(context.sp(), Ordering::SeqCst);
    H_FRAME.store(frame, Ordering::SeqCst);
    H_RET.store(ret, Ordering::SeqCst);
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
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

/// Read the installed disposition through the same private 40-byte wire.
fn sigaction_query(signo: i32, previous: &mut SigAction) -> isize {
    unsafe {
        syscall3(
            SIGACTION,
            signo as usize,
            0,
            previous as *mut SigAction as usize,
        )
    }
}

fn sigprocmask(how: usize, set: &u64) -> isize {
    unsafe { syscall3(SIGPROCMASK, how, set as *const u64 as usize, 0) }
}

/// Unblock `bit` with an inline syscall and return the interrupted `(pc, sp)`.
#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "aarch64")]
fn unblock_and_capture(bit: u64) -> (u64, u64) {
    let mut pc: u64;
    let mut sp: u64;
    // SAFETY: the syscall takes a valid u64 pointer; the label is the instruction after it.
    unsafe {
        asm!(
            "mov {sp}, sp",
            "adr {pc}, 2f",
            "svc #0",
            "2:",
            in("x8") SIGPROCMASK,
            in("x0") SIG_UNBLOCK,
            in("x1") core::ptr::addr_of!(bit),
            in("x2") 0usize,
            sp = out(reg) sp,
            pc = out(reg) pc,
        );
    }
    (pc, sp)
}

#[cfg(target_arch = "x86_64")]
fn action_handler() -> u64 {
    __s44_action_entry as *const () as u64
}

#[cfg(target_arch = "aarch64")]
fn action_handler() -> u64 {
    s44_action_body as *const () as u64
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("signal-action: start");
    if guard::run().is_err() {
        println!("signal-action: raw guard failed");
        sys::thread_exit(1);
    }
    println!("signal-action: raw guard ok");
    let action = SigAction {
        handler: action_handler(),
        mask: [0; 2],
        flags: SA_SIGINFO,
        _pad: 0,
        restorer: 0,
    };
    if sigaction(SIGUSR1, &action) != 0 {
        println!("signal-action: sigaction failed");
        sys::thread_exit(1);
    }
    let mut installed = SigAction::zero();
    if sigaction_query(SIGUSR1, &mut installed) != 0 {
        println!("signal-action: sigaction query failed");
        sys::thread_exit(1);
    }
    if installed.handler != action_handler() || installed.flags & SA_SIGINFO == 0 {
        println!("signal-action: installed flags wrong");
        sys::thread_exit(1);
    }
    println!("signal-action: sigaction ok");
    println!("signal-action: installed us1 flags=0x{:x}", installed.flags);

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
    #[cfg(target_arch = "x86_64")]
    if frame & 0xf != 8 || frame + SIGFRAME_SIZE as u64 > sp - 128 {
        println!("signal-action: frame placement wrong");
        sys::thread_exit(1);
    }
    #[cfg(target_arch = "aarch64")]
    if frame & 0xf != 0 || frame + SIGFRAME_SIZE as u64 > sp {
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
        handler: action_handler(),
        mask: [0; 2],
        flags: SA_SIGINFO | SA_ONSTACK,
        _pad: 0,
        restorer: 0,
    };
    if sigaction(SIGUSR2, &action) != 0 {
        println!("signal-action: alt sigaction failed");
        sys::thread_exit(1);
    }
    let mut installed = SigAction::zero();
    if sigaction_query(SIGUSR2, &mut installed) != 0 {
        println!("signal-action: alt sigaction query failed");
        sys::thread_exit(1);
    }
    if installed.handler != action_handler()
        || installed.flags & (SA_SIGINFO | SA_ONSTACK) != SA_SIGINFO | SA_ONSTACK
    {
        println!("signal-action: alt installed flags wrong");
        sys::thread_exit(1);
    }
    println!("signal-action: alt sigaction ok");
    println!("signal-action: installed us2 flags=0x{:x}", installed.flags);
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
    if alt_frame < alt_start || alt_frame + SIGFRAME_SIZE as u64 > alt_start + ALT_SIZE as u64 {
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
