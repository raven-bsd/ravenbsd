// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S44 signal-entry fixture.
//!
//! Exercises self-kick delivery through `SIG_SETUP`, a blocked futex waiter
//! kicked by another thread, the no-registration `-EINTR` fallback,
//! `SIGRETURN` outside a signal, and `SIG_SETUP` argument validation. The user
//! entry reads the saved C36 context from the TLS area and returns through
//! `SIGRETURN`; the interrupted instruction is identified by a label address
//! captured after the syscall.

#![no_std]
#![no_main]

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{self, MContext};
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const CANCEL: u32 = 1 << 1;
const CHECK: u32 = 1 << 0;
const PAGE: usize = 4096;
const CTX_OFF: usize = 0x100;
const SCENARIO_SELF: u32 = 1;
const SCENARIO_BLOCKED: u32 = 2;

static SCENARIO: AtomicU32 = AtomicU32::new(0);
static ENTRY_COUNT: AtomicU32 = AtomicU32::new(0);
static SELF_BITS: AtomicU32 = AtomicU32::new(0);
static SELF_CTX_PC: AtomicU64 = AtomicU64::new(0);
static SELF_CTX_SP: AtomicU64 = AtomicU64::new(0);
static BLOCK_WORD: AtomicU32 = AtomicU32::new(0);

// The kernel enters the registered point with a 16-byte-aligned stack. The
// SysV ABI expects RSP % 16 == 8 at a Rust function entry, so the x86 stub
// aligns and uses `call`. The aarch64 ABI already requires SP to be 16-byte
// aligned, and `bl` leaves it in place.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global __s44_sigentry
    .type __s44_sigentry, @function
__s44_sigentry:
    and rsp, -16
    call s44_signal_body
    ud2
    .size __s44_sigentry, . - __s44_sigentry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global __s44_sigentry
    .type __s44_sigentry, %function
__s44_sigentry:
    bl s44_signal_body
    brk #0
    .size __s44_sigentry, . - __s44_sigentry
"#
);

unsafe extern "C" {
    fn __s44_sigentry();
}

fn signal_entry_address() -> usize {
    __s44_sigentry as *const () as usize
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe {
        asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack),
        );
    }
    result
}

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn set_tls(tp: usize) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            raven_abi::syscall::THREAD_SET_TLS,
            Handle::NONE.0 as usize,
            tp,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn sig_setup(entry: usize, offset: usize, flags: u32) -> Result<(), Errno> {
    result(unsafe { syscall6(signal::SIG_SETUP, entry, offset, flags as usize, 0, 0, 0) })
        .map(|_| ())
}

fn sigreturn() -> isize {
    unsafe { syscall6(signal::SIGRETURN, 0, 0, 0, 0, 0, 0) }
}

fn kick(thread: Handle, bits: u32) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            raven_abi::syscall::THREAD_KICK,
            thread.0 as usize,
            bits as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

#[cfg(target_arch = "x86_64")]
fn read_tls_base() -> u64 {
    let base: u64;
    // SAFETY: FSGSBASE is enabled by the kernel when the CPU advertises it.
    unsafe {
        asm!("rdfsbase {}", out(reg) base, options(nomem, nostack, preserves_flags));
    }
    base
}

#[cfg(target_arch = "aarch64")]
fn read_tls_base() -> u64 {
    let base: u64;
    // SAFETY: TPIDR_EL0 is the user TLS base register.
    unsafe {
        asm!("mrs {}, tpidr_el0", out(reg) base, options(nomem, nostack, preserves_flags));
    }
    base
}

#[cfg(target_arch = "x86_64")]
fn read_sp() -> u64 {
    let sp: u64;
    // SAFETY: reads the current stack pointer; no memory is touched.
    unsafe {
        asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack, preserves_flags));
    }
    sp
}

#[cfg(target_arch = "aarch64")]
fn read_sp() -> u64 {
    let sp: u64;
    // SAFETY: reads the current stack pointer; no memory is touched.
    unsafe {
        asm!("mov {}, sp", out(reg) sp, options(nomem, nostack, preserves_flags));
    }
    sp
}

fn main(_args: raven_rt::Args) -> i32 {
    controller()
}

fn controller() -> i32 {
    let tls = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    if set_tls(tls).is_err() {
        println!("signal-entry: tls failed");
        return 1;
    }
    // The TLS base is installed by the next context switch; the entry reads it
    // back through the hardware register.
    let _ = sys::nanosleep(2_000_000);
    if sig_setup(signal_entry_address(), CTX_OFF, 0).is_err() {
        println!("signal-entry: setup failed");
        return 1;
    }
    println!("signal-entry: setup=0");

    match sigreturn() {
        raw if raw == Errno::EINVAL.to_ret() => println!("signal-entry: sigreturn-outside=EINVAL"),
        _ => {
            println!("signal-entry: sigreturn-outside unexpected");
            return 1;
        }
    }

    let data = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    match sig_setup(0, CTX_OFF, 0) {
        Err(Errno::EFAULT) => println!("signal-entry: bad entry=EFAULT"),
        _ => return 1,
    }
    match sig_setup(data, CTX_OFF, 0) {
        Err(Errno::EFAULT) => println!("signal-entry: bad data-entry=EFAULT"),
        _ => return 1,
    }
    match sig_setup(signal_entry_address(), 8, 0) {
        Err(Errno::EINVAL) => println!("signal-entry: bad offset=EINVAL"),
        _ => return 1,
    }
    match sig_setup(signal_entry_address(), 0xff10, 0) {
        Err(Errno::EINVAL) => println!("signal-entry: bad limit=EINVAL"),
        _ => return 1,
    }
    match sig_setup(signal_entry_address(), CTX_OFF, 2) {
        Err(Errno::EINVAL) => println!("signal-entry: bad flags=EINVAL"),
        _ => return 1,
    }
    match sig_setup(signal_entry_address(), CTX_OFF, signal::ENTRY_SP) {
        Ok(()) => println!("signal-entry: flag entry-sp=0"),
        _ => return 1,
    }
    match sig_setup(signal_entry_address(), CTX_OFF, 0) {
        Ok(()) => println!("signal-entry: setup replace=0"),
        _ => return 1,
    }

    let no_setup_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        no_setup as *const () as usize,
        no_setup_stack + 2 * PAGE,
        0,
    );
    let _ = sys::nanosleep(200_000_000);
    if ENTRY_COUNT.load(Ordering::SeqCst) == 0 {
        println!("signal-entry: no-setup entries=0");
    } else {
        println!("signal-entry: no-setup unexpected entry");
        return 1;
    }

    SCENARIO.store(SCENARIO_SELF, Ordering::SeqCst);
    let (raw, sp_before, after_pc) = unsafe { self_kick_capture() };
    if raw != 0 {
        println!("signal-entry: self kick failed raw={raw}");
        return 1;
    }
    let pc_match = SELF_CTX_PC.load(Ordering::SeqCst) == after_pc;
    let sp_match = SELF_CTX_SP.load(Ordering::SeqCst) == sp_before;
    let bits_match = SELF_BITS.load(Ordering::SeqCst) == CANCEL;
    println!(
        "signal-entry: self resume pc=0x{after_pc:016x} sp=0x{sp_before:016x} pc-match={} sp-match={} bits-match={}",
        u8::from(pc_match),
        u8::from(sp_match),
        u8::from(bits_match)
    );
    if !pc_match || !sp_match || !bits_match {
        return 1;
    }

    let blocked_tls = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    let blocked_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let blocked = match sys::thread_create(
        Handle::SELF_PROC,
        blocked_waiter as *const () as usize,
        blocked_stack + 2 * PAGE,
        blocked_tls,
    ) {
        Ok(handle) => handle,
        Err(_) => return 1,
    };
    let kicker_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        kicker as *const () as usize,
        kicker_stack + 2 * PAGE,
        blocked.0 as usize,
    );
    let _ = sys::nanosleep(500_000_000);
    println!("signal-entry: done");
    0
}

/// Self-kick with the syscall issued from this frame, so the interrupted SP
/// and the address after the syscall are known exactly.
#[cfg(target_arch = "x86_64")]
unsafe fn self_kick_capture() -> (isize, u64, u64) {
    let sp_before: usize;
    let after: usize;
    let raw: isize;
    // SAFETY: the syscall ABI is fixed; the kernel saves and restores all GPRs
    // around the signal entry, and the label instruction captures the resume PC.
    unsafe {
        asm!(
            "mov {sp}, rsp",
            "syscall",
            "2:",
            "lea {after}, [rip + 2b]",
            sp = out(reg) sp_before,
            after = out(reg) after,
            inlateout("rax") raven_abi::syscall::THREAD_KICK => raw,
            in("rdi") Handle::NONE.0 as usize,
            in("rsi") CANCEL as usize,
            in("rdx") 0usize,
            in("r10") 0usize,
            in("r8") 0usize,
            in("r9") 0usize,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    (raw, sp_before as u64, after as u64)
}

#[cfg(target_arch = "aarch64")]
unsafe fn self_kick_capture() -> (isize, u64, u64) {
    let sp_before: usize;
    let after: usize;
    let raw: isize;
    // SAFETY: the SVC ABI is fixed; the kernel saves and restores all GPRs
    // around the signal entry, and ADR captures the resume PC.
    unsafe {
        asm!(
            "mov {sp}, sp",
            "svc #0",
            "adr {after}, .",
            sp = out(reg) sp_before,
            after = out(reg) after,
            in("x8") raven_abi::syscall::THREAD_KICK,
            inlateout("x0") Handle::NONE.0 as usize => raw,
            in("x1") CANCEL as usize,
            in("x2") 0usize,
            in("x3") 0usize,
            in("x4") 0usize,
            in("x5") 0usize,
            options(nostack),
        );
    }
    (raw, sp_before as u64, after as u64)
}

#[unsafe(no_mangle)]
extern "C" fn s44_signal_body(bits: usize) -> ! {
    let scenario = SCENARIO.load(Ordering::SeqCst);
    let count = ENTRY_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    let base = read_tls_base();
    let context_address = base.wrapping_add(CTX_OFF as u64);
    // SAFETY: the kernel registered this area through SIG_SETUP and wrote a
    // full MContext there before entering; the TLS page is mapped by the test.
    let context = unsafe { &*(context_address as *const MContext) };
    let entry_sp = read_sp();
    println!(
        "signal-entry: entry scenario={} bits={} count={} pc=0x{:016x} sp=0x{:016x} entry-sp=0x{:016x}",
        scenario,
        bits,
        count,
        context.pc(),
        context.sp(),
        entry_sp
    );
    #[cfg(target_arch = "x86_64")]
    let saved_result = context.reg(signal::REG_RAX).unwrap_or(0) as isize;
    #[cfg(target_arch = "aarch64")]
    let saved_result = context.reg(0).unwrap_or(0) as isize;
    println!(
        "signal-entry: context size={} pc=0x{:016x} sp=0x{:016x} result={}",
        size_of::<MContext>(),
        context.pc(),
        context.sp(),
        saved_result
    );
    if scenario == SCENARIO_SELF {
        SELF_BITS.store(bits as u32, Ordering::SeqCst);
        SELF_CTX_PC.store(context.pc(), Ordering::SeqCst);
        SELF_CTX_SP.store(context.sp(), Ordering::SeqCst);
    }
    let raw = sigreturn();
    println!("signal-entry: sigreturn returned raw={raw}");
    loop {
        core::hint::spin_loop();
    }
}

extern "C" fn blocked_waiter(arg: usize) -> ! {
    if set_tls(arg).is_err() {
        println!("signal-entry: blocked tls failed");
        sys::thread_exit(1);
    }
    let _ = sys::nanosleep(1_000_000);
    if sig_setup(signal_entry_address(), CTX_OFF, 0).is_err() {
        println!("signal-entry: blocked setup failed");
        sys::thread_exit(1);
    }
    SCENARIO.store(SCENARIO_BLOCKED, Ordering::SeqCst);
    println!("signal-entry: blocked ready");
    match sys::futex_wait(&BLOCK_WORD, 0, None) {
        Err(Errno::EINTR) => println!("signal-entry: blocked eintr=1"),
        _ => println!("signal-entry: blocked unexpected"),
    }
    sys::thread_exit(0)
}

extern "C" fn kicker(arg: usize) -> ! {
    let _ = sys::nanosleep(50_000_000);
    match kick(Handle(arg as u32), CANCEL) {
        Ok(()) => println!("signal-entry: blocked kick=0"),
        Err(_) => println!("signal-entry: blocked kick failed"),
    }
    sys::thread_exit(0)
}

extern "C" fn no_setup(_arg: usize) -> ! {
    let _ = kick(Handle::NONE, CHECK);
    match sys::nanosleep(1_000_000_000) {
        Err(Errno::EINTR) => println!("signal-entry: no-setup=eintr"),
        _ => println!("signal-entry: no-setup unexpected"),
    }
    sys::thread_exit(0)
}
