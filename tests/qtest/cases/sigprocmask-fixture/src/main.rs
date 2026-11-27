// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S166 fixture: the per-thread signal mask.
//!
//! Three rules from CONTRACTS.md C19 / POSIX / FreeBSD `kern_sig.c`:
//! 1. a mask set on one thread is invisible to a sibling, while `THREAD_CREATE` inherits the
//!    creator's mask at creation (`pthread_create.3`: "The signal mask is inherited from the
//!    creating thread");
//! 2. a blocked signal stays pending and is delivered only after the unblock;
//! 3. `SIGRETURN` restores the `uc_sigmask` from the frame exactly, including a handler edit.
//!
//! The fixture uses the raw 40-byte `SIGACTION` request and raw `PROC_SIGNAL` so the kernel path
//! is exercised without libc `raise` (K571).

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, ThreadCreateEx};
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const SIGUSR1: usize = 10;
const SIGUSR2: usize = 12;
const SIG_BLOCK: usize = 1;
const SIG_SETMASK: usize = 3;
const SIGPROCMASK: usize = raven_abi::syscall::SIGPROCMASK;
const SIGACTION: usize = raven_abi::syscall::SIGACTION;
/// `proc_signal(target, signo)` raw number (`kernel/src/syscall/process_ctl.rs:186`); the lead
/// owns the `raven-abi` variant.
const PROC_SIGNAL: usize = 0x4A;

const BIT_US1: u64 = 1 << (SIGUSR1 - 1);
const BIT_US2: u64 = 1 << (SIGUSR2 - 1);

/// The Raven 40-byte `SIGACTION` request (`raven-libc-signal/src/sigaction_abi.rs`).
#[repr(C)]
struct KernelSigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

const _: () = assert!(core::mem::size_of::<KernelSigAction>() == 40);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, mask) == 8);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, flags) == 24);
const _: () = assert!(core::mem::offset_of!(KernelSigAction, restorer) == 32);

/// System V amd64 `ucontext_t` (936 bytes); only `uc_sigmask` at offset 296 is used here.
#[repr(C)]
struct UContext {
    flags: u64,
    link: u64,
    stack: [u64; 3],
    mcontext: [u64; 32],
    sigmask: [u64; 16],
    fpregs: [u64; 64],
}

const _: () = assert!(core::mem::size_of::<UContext>() == 936);
const _: () = assert!(core::mem::offset_of!(UContext, sigmask) == 296);

static HANDLED_US1: AtomicU32 = AtomicU32::new(0);
static HANDLED_US2: AtomicU32 = AtomicU32::new(0);
static HANDLER_UC_MASK: AtomicU64 = AtomicU64::new(u64::MAX);
static HANDLER_WRITE_MASK: AtomicU64 = AtomicU64::new(u64::MAX);
static CHILD_MASK: AtomicU64 = AtomicU64::new(u64::MAX);
static CHILD_MASK_AFTER: AtomicU64 = AtomicU64::new(u64::MAX);
static CHILD_TID: AtomicU32 = AtomicU32::new(0);
static CHILD_READY: AtomicBool = AtomicBool::new(false);
static CHILD_DONE: AtomicBool = AtomicBool::new(false);
static MAIN_QUERIED: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s166_handler_entry
    .type s166_handler_entry, @function
s166_handler_entry:
    mov r12, rsp
    mov rcx, r12
    call s166_handler_body
    mov rsp, r12
    ret
    .size s166_handler_entry, . - s166_handler_entry
"#
);

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn s166_handler_entry();
}

#[unsafe(no_mangle)]
extern "C" fn s166_handler_body(sig: i32, _info: *const u8, context: *mut UContext) {
    let context = unsafe { &mut *context };
    match sig as usize {
        SIGUSR1 => {
            HANDLED_US1.fetch_add(1, Ordering::SeqCst);
            HANDLER_UC_MASK.store(context.sigmask[0], Ordering::SeqCst);
            let write = HANDLER_WRITE_MASK.load(Ordering::SeqCst);
            if write != u64::MAX {
                context.sigmask[0] = write;
            }
        }
        SIGUSR2 => {
            HANDLED_US2.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }
}

fn checked(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn install(signo: usize, handler: usize) -> Result<(), Errno> {
    let action = KernelSigAction {
        handler: handler as u64,
        mask: [0; 2],
        flags: 0,
        _pad: 0,
        restorer: 0,
    };
    // SAFETY: SIGACTION copies the 40-byte live request; `signo` is a valid signal number.
    checked(unsafe {
        sys::syscall3(
            SIGACTION,
            signo,
            &action as *const KernelSigAction as usize,
            0,
        )
    })
    .map(|_| ())
}

fn sigprocmask(how: usize, set: Option<&u64>, old: Option<&mut u64>) -> Result<(), Errno> {
    let set_ptr = set.map_or(0, |value| value as *const u64 as usize);
    let old_ptr = old.map_or(0, |value| value as *mut u64 as usize);
    // SAFETY: SIGPROCMASK copies one u64 from/to the live pointers; NULL is allowed.
    checked(unsafe { sys::syscall3(SIGPROCMASK, how, set_ptr, old_ptr) }).map(|_| ())
}

fn query_mask() -> u64 {
    let mut current = 0u64;
    let _ = sigprocmask(SIG_BLOCK, None, Some(&mut current));
    current
}

fn proc_signal(signo: usize) -> Result<(), Errno> {
    // SAFETY: PROC_SIGNAL takes the target pid/group and a signal number; the kernel checks the
    // caller's permission. Target 0 is the caller's process group.
    checked(unsafe { sys::syscall2(PROC_SIGNAL, 0, signo) }).map(|_| ())
}

fn create_extended(
    entry: usize,
    stack_top: usize,
    arg: usize,
    extended: &ThreadCreateEx,
) -> Result<Handle, Errno> {
    // SAFETY: THREAD_CREATE copies the live ThreadCreateEx payload and validates every pointer.
    checked(unsafe {
        sys::syscall6(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            entry,
            stack_top,
            arg,
            THREAD_CREATE_EX,
            extended as *const ThreadCreateEx as usize,
        )
    })
    .map(|raw| Handle(raw as u32))
}

fn wait_flag(flag: &AtomicBool) -> bool {
    for _ in 0..50_000 {
        if flag.load(Ordering::Acquire) {
            return true;
        }
        let _ = sys::nanosleep(1_000_000);
        core::hint::spin_loop();
    }
    false
}

fn wait_word(word: &AtomicU64, expected: u64) -> bool {
    for _ in 0..50_000 {
        if word.load(Ordering::Acquire) != expected {
            return true;
        }
        let _ = sys::nanosleep(1_000_000);
        core::hint::spin_loop();
    }
    false
}

extern "C" fn child_entry(_arg: usize) -> ! {
    CHILD_MASK.store(query_mask(), Ordering::SeqCst);
    let own = BIT_US2;
    let _ = sigprocmask(SIG_SETMASK, Some(&own), None);
    CHILD_MASK_AFTER.store(query_mask(), Ordering::SeqCst);
    CHILD_READY.store(true, Ordering::Release);
    let _ = wait_flag(&MAIN_QUERIED);
    CHILD_DONE.store(true, Ordering::Release);
    sys::thread_exit(0)
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("sigprocmask: start");
    if install(SIGUSR1, s166_handler_entry as usize).is_err()
        || install(SIGUSR2, s166_handler_entry as usize).is_err()
    {
        println!("sigprocmask: install failed");
        return 1;
    }
    println!("sigprocmask: install ok");

    let mut failures = 0u32;

    // Rule 2: a blocked signal stays pending and is delivered only after the unblock.
    let blocked_mask = BIT_US2;
    if sigprocmask(SIG_SETMASK, Some(&blocked_mask), None).is_err() || proc_signal(SIGUSR2).is_err()
    {
        println!("sigprocmask: pending setup failed");
        return 1;
    }
    let blocked_now = query_mask();
    let pending_ok = HANDLED_US2.load(Ordering::SeqCst) == 0 && blocked_now & BIT_US2 != 0;
    println!(
        "sigprocmask: pending blocked={} delivered={}",
        u8::from(blocked_now & BIT_US2 != 0),
        HANDLED_US2.load(Ordering::SeqCst)
    );
    if !pending_ok {
        failures = failures.saturating_add(1);
    }
    let empty = 0u64;
    if sigprocmask(SIG_SETMASK, Some(&empty), None).is_err() {
        println!("sigprocmask: unblock failed");
        return 1;
    }
    let after_unblock = query_mask();
    let unblock_ok = HANDLED_US2.load(Ordering::SeqCst) == 1 && after_unblock == 0;
    println!(
        "sigprocmask: unblock delivered={} mask=0x{:x}",
        HANDLED_US2.load(Ordering::SeqCst),
        after_unblock
    );
    if !unblock_ok {
        failures = failures.saturating_add(1);
    }

    // Rule 3: SIGRETURN restores uc_sigmask exactly, including a handler edit.
    let pre_handler = BIT_US2;
    if sigprocmask(SIG_SETMASK, Some(&pre_handler), None).is_err() {
        println!("sigprocmask: restore setup failed");
        return 1;
    }
    let handler_mask = BIT_US1 | BIT_US2;
    HANDLER_WRITE_MASK.store(handler_mask, Ordering::SeqCst);
    if proc_signal(SIGUSR1).is_err() {
        println!("sigprocmask: restore send failed");
        return 1;
    }
    let captured = HANDLER_UC_MASK.load(Ordering::SeqCst);
    let after_handler = query_mask();
    let restore_ok = HANDLED_US1.load(Ordering::SeqCst) == 1
        && captured == pre_handler
        && after_handler == handler_mask;
    println!(
        "sigprocmask: restore captured=0x{:x} after=0x{:x} ok={}",
        captured,
        after_handler,
        u8::from(restore_ok)
    );
    if !restore_ok {
        failures = failures.saturating_add(1);
    }
    HANDLER_WRITE_MASK.store(u64::MAX, Ordering::SeqCst);
    if sigprocmask(SIG_SETMASK, Some(&empty), None).is_err() {
        println!("sigprocmask: restore reset failed");
        return 1;
    }

    // Rule 1: the creator's mask is inherited at THREAD_CREATE; a later sibling change is
    // invisible to the creator.
    let main_mask = BIT_US1;
    if sigprocmask(SIG_SETMASK, Some(&main_mask), None).is_err() {
        println!("sigprocmask: sibling setup failed");
        return 1;
    }
    let stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => {
            println!("sigprocmask: stack map failed");
            return 1;
        }
    };
    let extended = ThreadCreateEx {
        tp: 0,
        tid_out: &CHILD_TID as *const _ as usize,
        clear_tid_addr: 0,
    };
    let child = match create_extended(
        child_entry as *const () as usize,
        stack.saturating_add(2 * PAGE),
        0,
        &extended,
    ) {
        Ok(child) => child,
        Err(_) => {
            println!("sigprocmask: child create failed");
            return 1;
        }
    };
    if !wait_flag(&CHILD_READY) {
        println!("sigprocmask: child timeout");
        return 1;
    }
    let inherited = CHILD_MASK.load(Ordering::SeqCst);
    let inherit_ok = inherited & BIT_US1 != 0;
    println!(
        "sigprocmask: child inherit us1={} mask=0x{:x}",
        u8::from(inherit_ok),
        inherited
    );
    if !inherit_ok {
        failures = failures.saturating_add(1);
    }
    let child_changed = wait_word(&CHILD_MASK_AFTER, u64::MAX);
    let sibling_mask = CHILD_MASK_AFTER.load(Ordering::SeqCst);
    let main_now = query_mask();
    let invisible_ok = child_changed
        && main_now & BIT_US1 != 0
        && main_now & BIT_US2 == 0
        && sibling_mask & BIT_US2 != 0
        && sibling_mask & BIT_US1 == 0;
    println!(
        "sigprocmask: sibling main=0x{:x} child=0x{:x} invisible={}",
        main_now,
        sibling_mask,
        u8::from(invisible_ok)
    );
    if !invisible_ok {
        failures = failures.saturating_add(1);
    }
    MAIN_QUERIED.store(true, Ordering::Release);
    let _ = wait_flag(&CHILD_DONE);
    let _ = child;

    if failures != 0 {
        println!("sigprocmask: fail rows={failures}");
        return 1;
    }
    println!("sigprocmask: done");
    0
}
