// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S44/S161/S712 C19 fixture: kernel `sigaction`, the kernel `sendsig` frame, sigcode return,
//! `sigaltstack`, real signal nesting, FP/SIMD preservation and fork disposition inheritance on
//! both supported architectures.
//!
//! The fixture installs a `SA_SIGINFO` handler through the new kernel syscall, blocks and raises
//! `SIGUSR1`, then unblocks it while capturing the interrupted pc/sp. The handler checks the
//! signal arguments, the `ucontext_t` pc/sp, the frame address, and that its return address is the
//! page-aligned sigcode. It then returns through the sigcode and the interrupted label resumes.
//! A second scenario installs `SA_ONSTACK` and proves the frame lands on the alternate stack.
//!
//! S712 adds rows that a single in-handler boolean cannot pass: SIGUSR2 raised from inside the
//! SIGUSR1 handler must run before the raising syscall returns, `SA_NODEFER` must allow a nested
//! same-signal delivery, `SA_RESETHAND` must reset the disposition, the interrupted SSE/NEON
//! register must survive a handler that overwrites it, and a forked child's first thread must
//! inherit the parent's caught/ignored dispositions and signal mask before it executes anything.

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
const SA_NODEFER: u32 = 0x4000_0000;
const SA_RESETHAND: u32 = 0x8000_0000;
const SIG_DFL: u64 = 0;
const SIG_IGN: u64 = 1;
const SIG_BLOCK: usize = 0;
const SIG_UNBLOCK: usize = 1;
const SS_ONSTACK: u32 = 1;
const ALT_SIZE: usize = 8192;
const SIGFRAME_SIZE: usize = 0x410;
const SIGINFO_SIZE: usize = 0x50;

/// Raw `proc_fork(src, flags) -> child process handle` (`kernel/src/syscall/process_ctl.rs`).
const PROC_FORK: usize = 0x39;
/// Raw `proc_handle_info(handle) -> pid`.
const PROC_HANDLE_INFO: usize = 0x3E;
/// Raw `proc_wait_ex(idtype, id, options, out_status) -> pid`.
const PROC_WAIT_EX: usize = 0x48;
/// `P_PID` selector for `PROC_WAIT_EX`.
const P_PID: usize = 1;
/// `THREAD_CREATE` raw number (`raven_abi::syscall::Syscall`).
const THREAD_CREATE: usize = 0x20;

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

/// Stack the parent gives the forked child's first thread. It lives in the inherited `.bss`
/// mapping, which is writable in the child's copied address space.
#[repr(C, align(16))]
struct ChildStack([u8; 64 * 1024]);
static mut CHILD_STACK: ChildStack = ChildStack([0; 64 * 1024]);

static NESTED_OUTER: AtomicU32 = AtomicU32::new(0);
static NESTED_INNER: AtomicU32 = AtomicU32::new(0);
static NESTED_BEFORE_RETURN: AtomicU32 = AtomicU32::new(0);
static NODEFER_DEPTH: AtomicU32 = AtomicU32::new(0);
static NODEFER_MAX: AtomicU32 = AtomicU32::new(0);
static NODEFER_NESTED: AtomicU32 = AtomicU32::new(0);
static RESETHAND_RAN: AtomicU32 = AtomicU32::new(0);
static FORK_HANDLED: AtomicU32 = AtomicU32::new(0);
static FP_OBSERVED: AtomicU64 = AtomicU64::new(0);
static FP_HANDLER_RAN: AtomicU32 = AtomicU32::new(0);

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

/// SIGUSR1 handler for the nested-before-return row (S712 step 5).
///
/// SIGUSR2 is queued from inside this handler; the kernel must run its handler before this
/// syscall returns, so the flag is observable here, not only after the outer handler exits.
extern "C" fn nested_outer(_sig: i32) {
    NESTED_OUTER.fetch_add(1, Ordering::SeqCst);
    let _ = unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR2 as usize, 0) };
    if NESTED_INNER.load(Ordering::SeqCst) == 1 {
        NESTED_BEFORE_RETURN.store(1, Ordering::SeqCst);
    }
}

/// SIGUSR2 handler that runs inside the SIGUSR1 handler.
extern "C" fn nested_inner(_sig: i32) {
    NESTED_INNER.fetch_add(1, Ordering::SeqCst);
}

/// SIGUSR1 handler for the `SA_NODEFER` same-signal nesting row.
extern "C" fn nodefer_handler(_sig: i32) {
    let depth = NODEFER_DEPTH.fetch_add(1, Ordering::SeqCst) + 1;
    NODEFER_MAX.fetch_max(depth, Ordering::SeqCst);
    if depth == 2 {
        NODEFER_NESTED.store(1, Ordering::SeqCst);
    }
    if depth == 1 {
        // SA_NODEFER keeps SIGUSR1 unblocked inside the handler; this must nest.
        let _ = unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) };
    }
    NODEFER_DEPTH.fetch_sub(1, Ordering::SeqCst);
}

/// SIGUSR1 handler for the `SA_RESETHAND` row.
extern "C" fn resethand_handler(_sig: i32) {
    RESETHAND_RAN.fetch_add(1, Ordering::SeqCst);
}

/// SIGUSR1 handler used to prove fork disposition inheritance.
extern "C" fn fork_handler(_sig: i32) {
    FORK_HANDLED.fetch_add(1, Ordering::SeqCst);
}

fn fork_handler_address() -> u64 {
    fork_handler as *const () as u64
}

/// SIGUSR1 handler that overwrites the interrupted SSE/NEON register.
extern "C" fn fp_handler(_sig: i32) {
    FP_HANDLER_RAN.fetch_add(1, Ordering::SeqCst);
    // SAFETY: the register transfer is a single instruction and does not touch memory.
    unsafe { fp_write(0x4000_0000_0000_0000) }; // 2.0
}

/// Load `bits` into the low 64 bits of the first SIMD register.
#[cfg(target_arch = "x86_64")]
unsafe fn fp_write(bits: u64) {
    // SAFETY: MOVQ transfers one 64-bit value between a GPR and XMM0; no memory is accessed.
    unsafe {
        asm!("movq xmm0, {}", in(reg) bits, options(nostack, preserves_flags));
    }
}

/// Load `bits` into the low 64 bits of the first SIMD register.
#[cfg(target_arch = "aarch64")]
unsafe fn fp_write(bits: u64) {
    // SAFETY: FMOV transfers one 64-bit value between a GPR and D0; no memory is accessed.
    unsafe {
        asm!(
            ".arch armv8-a+fp+simd",
            "fmov d0, {bits}",
            bits = in(reg) bits,
            options(nostack, preserves_flags),
        );
    }
}

/// Seed the SIMD register, queue `signo` to self, and read the register back after the kernel
/// has delivered and returned from the handler. One asm block so the compiler cannot spill or
/// reload the register between the three steps.
#[cfg(target_arch = "x86_64")]
unsafe fn queue_and_read_fp(signo: usize, seed: u64) -> u64 {
    let observed: u64;
    // SAFETY: the block performs one register move, the documented `PROC_SIGQUEUE` syscall and
    // one register move; the syscall clobbers only rax/rcx/r11 and is given explicit arguments.
    unsafe {
        asm!(
            "movq xmm0, {seed}",
            "syscall",
            "movq {observed}, xmm0",
            seed = in(reg) seed,
            inlateout("rax") PROC_SIGQUEUE => _,
            in("rdi") 0usize,
            in("rsi") signo,
            in("rdx") 0usize,
            observed = out(reg) observed,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    observed
}

/// Seed the SIMD register, queue `signo` to self, and read the register back after the kernel
/// has delivered and returned from the handler.
#[cfg(target_arch = "aarch64")]
unsafe fn queue_and_read_fp(signo: usize, seed: u64) -> u64 {
    let observed: u64;
    // SAFETY: the block performs one register move, the documented `PROC_SIGQUEUE` syscall and
    // one register move; the syscall clobbers only x0 and is given explicit arguments.
    unsafe {
        asm!(
            ".arch armv8-a+fp+simd",
            "fmov d0, {seed}",
            "svc #0",
            "fmov {observed}, d0",
            seed = in(reg) seed,
            in("x8") PROC_SIGQUEUE,
            inlateout("x0") 0usize => _,
            in("x1") signo,
            in("x2") 0usize,
            observed = out(reg) observed,
            options(nostack),
        );
    }
    observed
}

/// First thread of the forked child: prove inherited dispositions and mask, then deliver one
/// inherited handler and return the outcome through the process exit status.
extern "C" fn fork_child_entry(_arg: usize) -> ! {
    let mut us1 = SigAction::zero();
    let mut us2 = SigAction::zero();
    let mut mask = 0u64;
    let q1 = sigaction_query(SIGUSR1, &mut us1);
    let q2 = sigaction_query(SIGUSR2, &mut us2);
    // NULL set with a live old-set pointer is a pure read of the thread mask.
    let q3 = unsafe { syscall3(SIGPROCMASK, 0, 0, &mut mask as *mut u64 as usize) };
    let blocked_bit = 1u64 << (SIGUSR2 - 1);
    let inherited = q1 == 0
        && q2 == 0
        && q3 == 0
        && us1.handler == fork_handler_address()
        && us2.handler == SIG_IGN
        && mask & blocked_bit != 0;
    if inherited {
        let _ = unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) };
    }
    let delivered = FORK_HANDLED.load(Ordering::SeqCst) == 1;
    if inherited && delivered {
        sys::thread_exit(0);
    }
    println!(
        "signal-action: fork-child failed us1={} us2={} mask=0x{:x} handled={}",
        us1.handler,
        us2.handler,
        mask,
        FORK_HANDLED.load(Ordering::SeqCst)
    );
    sys::thread_exit(1)
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall1(number: usize, a0: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall1(number: usize, a0: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall4(number: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall4(number: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall5(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall5(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates every pointer.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            options(nostack),
        );
    }
    result
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

/// Wait for `pid`, retrying when the wait itself is interrupted by the child's `SIGCHLD` kick.
fn wait_child(pid: usize, status: &mut i32) -> isize {
    let mut attempts = 0u32;
    loop {
        // SAFETY: `status` is a live writable i32 and `pid` names the caller's child.
        let result = unsafe { syscall4(PROC_WAIT_EX, P_PID, pid, 0, status as *mut i32 as usize) };
        // `EINTR` is 4; a child-exit notification can race the status becoming visible.
        if result != -4 || attempts >= 16 {
            return result;
        }
        attempts += 1;
    }
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

    // S712 step 1: the trampoline mapping the frame returns through. The address is page
    // aligned and its bytes are the C19 sigcode, not a user-supplied entry.
    {
        let ret = H_RET.load(Ordering::SeqCst);
        #[cfg(target_arch = "x86_64")]
        let expected: &[u8] = &[0xB8, 0x82, 0x00, 0x00, 0x00, 0x0F, 0x05, 0x0F, 0x0B];
        #[cfg(target_arch = "aarch64")]
        let expected: &[u8] = &[0x48, 0x10, 0x80, 0xD2, 0x01, 0x00, 0x00, 0xD4];
        let mut bytes = [0u8; 9];
        let mut same = true;
        for (index, expected_byte) in expected.iter().enumerate() {
            // SAFETY: `ret` is the user-visible sigcode page base the kernel installed; the
            // page is mapped readable and the read stays inside its first bytes.
            bytes[index] = unsafe { core::ptr::read_volatile((ret as usize + index) as *const u8) };
            if bytes[index] != *expected_byte {
                same = false;
            }
        }
        if ret & 0xfff != 0 || !same {
            println!(
                "signal-action: sigcode failed addr=0x{:x} bytes={:02x?}",
                ret, bytes
            );
            sys::thread_exit(1);
        }
        println!("signal-action: sigcode ok addr=0x{:x}", ret);
    }

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

    // S712 step 5a: a signal raised from inside a handler runs before the raising syscall
    // returns. A counter read after the outer handler returns cannot prove this: the inner
    // handler sets the flag that the outer handler inspects *after* the raising call.
    {
        let outer = SigAction {
            handler: nested_outer as *const () as u64,
            mask: [0; 2],
            flags: SA_SIGINFO,
            _pad: 0,
            restorer: 0,
        };
        let inner = SigAction {
            handler: nested_inner as *const () as u64,
            mask: [0; 2],
            flags: SA_SIGINFO,
            _pad: 0,
            restorer: 0,
        };
        NESTED_OUTER.store(0, Ordering::SeqCst);
        NESTED_INNER.store(0, Ordering::SeqCst);
        NESTED_BEFORE_RETURN.store(0, Ordering::SeqCst);
        if sigaction(SIGUSR1, &outer) != 0 || sigaction(SIGUSR2, &inner) != 0 {
            println!("signal-action: nested sigaction failed");
            sys::thread_exit(1);
        }
        if unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) } != 0 {
            println!("signal-action: nested sigqueue failed");
            sys::thread_exit(1);
        }
        if NESTED_OUTER.load(Ordering::SeqCst) != 1
            || NESTED_INNER.load(Ordering::SeqCst) != 1
            || NESTED_BEFORE_RETURN.load(Ordering::SeqCst) != 1
        {
            println!(
                "signal-action: nested failed outer={} inner={} before_return={}",
                NESTED_OUTER.load(Ordering::SeqCst),
                NESTED_INNER.load(Ordering::SeqCst),
                NESTED_BEFORE_RETURN.load(Ordering::SeqCst)
            );
            sys::thread_exit(1);
        }
        println!("signal-action: nested ok");
    }

    // S712 step 5b: SA_NODEFER allows the same signal to nest inside its own handler. Without it
    // the signal is blocked by delivery and must stay pending.
    {
        let action = SigAction {
            handler: nodefer_handler as *const () as u64,
            mask: [0; 2],
            flags: SA_SIGINFO | SA_NODEFER,
            _pad: 0,
            restorer: 0,
        };
        NODEFER_DEPTH.store(0, Ordering::SeqCst);
        NODEFER_MAX.store(0, Ordering::SeqCst);
        NODEFER_NESTED.store(0, Ordering::SeqCst);
        if sigaction(SIGUSR1, &action) != 0 {
            println!("signal-action: nodefer sigaction failed");
            sys::thread_exit(1);
        }
        let bit = 1u64 << (SIGUSR1 - 1);
        if sigprocmask(SIG_BLOCK, &bit) != 0 {
            println!("signal-action: nodefer block failed");
            sys::thread_exit(1);
        }
        if unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) } != 0 {
            println!("signal-action: nodefer sigqueue failed");
            sys::thread_exit(1);
        }
        if NODEFER_DEPTH.load(Ordering::SeqCst) != 0 {
            println!("signal-action: nodefer delivered while blocked");
            sys::thread_exit(1);
        }
        if sigprocmask(SIG_UNBLOCK, &bit) != 0 {
            println!("signal-action: nodefer unblock failed");
            sys::thread_exit(1);
        }
        if NODEFER_MAX.load(Ordering::SeqCst) != 2
            || NODEFER_NESTED.load(Ordering::SeqCst) != 1
            || NODEFER_DEPTH.load(Ordering::SeqCst) != 0
        {
            println!(
                "signal-action: nodefer failed max={} nested={} depth={}",
                NODEFER_MAX.load(Ordering::SeqCst),
                NODEFER_NESTED.load(Ordering::SeqCst),
                NODEFER_DEPTH.load(Ordering::SeqCst)
            );
            sys::thread_exit(1);
        }
        println!("signal-action: nodefer ok");
    }

    // S712 step 5c: SA_RESETHAND resets the disposition to SIG_DFL before the handler runs.
    {
        let action = SigAction {
            handler: resethand_handler as *const () as u64,
            mask: [0; 2],
            flags: SA_SIGINFO | SA_RESETHAND,
            _pad: 0,
            restorer: 0,
        };
        RESETHAND_RAN.store(0, Ordering::SeqCst);
        if sigaction(SIGUSR1, &action) != 0 {
            println!("signal-action: resethand sigaction failed");
            sys::thread_exit(1);
        }
        if unsafe { syscall3(PROC_SIGQUEUE, 0, SIGUSR1 as usize, 0) } != 0 {
            println!("signal-action: resethand sigqueue failed");
            sys::thread_exit(1);
        }
        let mut restored = SigAction::zero();
        let queried = sigaction_query(SIGUSR1, &mut restored);
        if RESETHAND_RAN.load(Ordering::SeqCst) != 1 || queried != 0 || restored.handler != SIG_DFL
        {
            println!(
                "signal-action: resethand failed ran={} handler={} query={}",
                RESETHAND_RAN.load(Ordering::SeqCst),
                restored.handler,
                queried
            );
            sys::thread_exit(1);
        }
        println!("signal-action: resethand ok");
    }

    // S712 step 7: the interrupted SIMD register survives a handler that overwrites it.
    {
        let action = SigAction {
            handler: fp_handler as *const () as u64,
            mask: [0; 2],
            flags: SA_SIGINFO,
            _pad: 0,
            restorer: 0,
        };
        FP_HANDLER_RAN.store(0, Ordering::SeqCst);
        FP_OBSERVED.store(0, Ordering::SeqCst);
        if sigaction(SIGUSR1, &action) != 0 {
            println!("signal-action: fp sigaction failed");
            sys::thread_exit(1);
        }
        // 1.0 in IEEE-754 binary64.
        const SEED: u64 = 0x3ff0_0000_0000_0000;
        // SAFETY: the helper seeds and reads one SIMD register around a documented syscall.
        let observed = unsafe { queue_and_read_fp(SIGUSR1 as usize, SEED) };
        FP_OBSERVED.store(observed, Ordering::SeqCst);
        if FP_HANDLER_RAN.load(Ordering::SeqCst) != 1 || observed != SEED {
            println!(
                "signal-action: fp failed ran={} observed=0x{:x}",
                FP_HANDLER_RAN.load(Ordering::SeqCst),
                observed
            );
            sys::thread_exit(1);
        }
        println!("signal-action: fp ok observed=0x{:x}", observed);
    }

    // S712 step 6: SIGRETURN outside a delivered frame is rejected, not accepted from a
    // user-forged "in handler" state.
    {
        let result = unsafe { syscall1(raven_abi::signal::SIGRETURN, 0) };
        if result >= 0 {
            println!("signal-action: sigreturn-invalid failed result={}", result);
            sys::thread_exit(1);
        }
        println!("signal-action: sigreturn-invalid ok errno={}", -result);
    }

    // S712 step 3: a forked child's first thread inherits caught and ignored dispositions and
    // the calling thread's mask before it executes a single user instruction.
    {
        let caught = SigAction {
            handler: fork_handler_address(),
            mask: [0; 2],
            flags: SA_SIGINFO,
            _pad: 0,
            restorer: 0,
        };
        let ignored = SigAction {
            handler: SIG_IGN,
            mask: [0; 2],
            flags: 0,
            _pad: 0,
            restorer: 0,
        };
        FORK_HANDLED.store(0, Ordering::SeqCst);
        if sigaction(SIGUSR1, &caught) != 0 || sigaction(SIGUSR2, &ignored) != 0 {
            println!("signal-action: fork sigaction failed");
            sys::thread_exit(1);
        }
        let blocked = 1u64 << (SIGUSR2 - 1);
        if sigprocmask(SIG_BLOCK, &blocked) != 0 {
            println!("signal-action: fork block failed");
            sys::thread_exit(1);
        }
        println!("signal-action: fork armed");
        let child = unsafe { syscall2(PROC_FORK, 0, 0) };
        if child < 0 {
            println!("signal-action: fork failed errno={}", -child);
            sys::thread_exit(1);
        }
        println!("signal-action: fork forked child={}", child);
        let child_handle = child as usize;
        let pid = unsafe { syscall1(PROC_HANDLE_INFO, child_handle) };
        // SAFETY: `CHILD_STACK` is a live writable static in the image the child inherited.
        let stack_top =
            core::ptr::addr_of!(CHILD_STACK) as usize + core::mem::size_of::<ChildStack>();
        let created = unsafe {
            syscall5(
                THREAD_CREATE,
                child_handle,
                fork_child_entry as *const () as usize,
                stack_top,
                0,
                0,
            )
        };
        println!("signal-action: fork thread created={}", created);
        let mut status = 0i32;
        let waited = if created >= 0 && pid > 0 {
            wait_child(pid as usize, &mut status)
        } else {
            -1
        };
        if created >= 0 {
            let _ = sys::handle_close(raven_abi::types::Handle(created as u32));
        }
        let _ = sys::handle_close(raven_abi::types::Handle(child_handle as u32));
        let _ = sigprocmask(SIG_UNBLOCK, &blocked);
        if waited != pid || status != 0 {
            println!(
                "signal-action: fork failed wait={} pid={} status=0x{:x} created={}",
                waited, pid, status, created
            );
            sys::thread_exit(1);
        }
        println!("signal-action: fork ok pid={}", pid);
    }

    println!("signal-action: done");
    sys::thread_exit(0)
}
