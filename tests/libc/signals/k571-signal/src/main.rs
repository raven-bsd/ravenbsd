// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! K571 no-brush acceptance: install a caught handler through the public `sigaction`, raise it,
//! check the interrupted `rip`/`rsp` in the kernel-built `ucontext_t`, and return exactly.
//!
//! The kernel owns the frame and the sigcode (C19), so the saved `rip` is the instruction after
//! the libc `raise` syscall inside the linked text image, not an address near `&raise`.

use core::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

const SIGUSR1: c_int = 10;
const SA_SIGINFO: c_int = 4;

/// The public 128-byte `sigset_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigset {
    bits: [u64; 16],
}

/// The public 152-byte C `struct sigaction`: handler 0, mask 8, flags 136, restorer 144.
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigaction {
    handler: usize,
    mask: Sigset,
    flags: c_int,
    _pad: c_int,
    restorer: usize,
}

const _: () = assert!(core::mem::size_of::<Sigset>() == 128);
const _: () = assert!(core::mem::size_of::<Sigaction>() == 152);
const _: () = assert!(core::mem::offset_of!(Sigaction, handler) == 0);
const _: () = assert!(core::mem::offset_of!(Sigaction, mask) == 8);
const _: () = assert!(core::mem::offset_of!(Sigaction, flags) == 136);
const _: () = assert!(core::mem::offset_of!(Sigaction, restorer) == 144);

/// `ucontext_t.uc_mcontext.gregs[REG_RSP]` in the public x86-64 layout.
const UC_RSP_OFFSET: usize = 40 + 15 * 8;
/// `ucontext_t.uc_mcontext.gregs[REG_RIP]` in the public x86-64 layout.
const UC_RIP_OFFSET: usize = 40 + 16 * 8;

static RAN: AtomicI32 = AtomicI32::new(0);
static SEEN_SIG: AtomicI32 = AtomicI32::new(0);
static INTERRUPTED_RIP: AtomicUsize = AtomicUsize::new(0);
static INTERRUPTED_RSP: AtomicUsize = AtomicUsize::new(0);
static AFTER_RAISE: AtomicI32 = AtomicI32::new(0);

unsafe extern "C" {
    fn sigaction(sig: c_int, act: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn raise(sig: c_int) -> c_int;
}

fn read_word(context: *const u8, offset: usize) -> usize {
    // SAFETY: `context` is the live kernel `ucontext_t`; the offset names an in-bounds `gregs`
    // word of the public layout.
    unsafe { core::ptr::read_volatile(context.add(offset) as *const usize) }
}

extern "C" fn handler(signal_number: c_int, _info: *mut c_void, context: *mut c_void) {
    RAN.fetch_add(1, Ordering::SeqCst);
    SEEN_SIG.store(signal_number, Ordering::SeqCst);
    if context.is_null() {
        return;
    }
    let context = context as *const u8;
    INTERRUPTED_RIP.store(read_word(context, UC_RIP_OFFSET), Ordering::SeqCst);
    INTERRUPTED_RSP.store(read_word(context, UC_RSP_OFFSET), Ordering::SeqCst);
}

fn install() -> bool {
    let action = Sigaction {
        handler: handler as *const () as usize,
        mask: Sigset { bits: [0; 16] },
        flags: SA_SIGINFO,
        _pad: 0,
        restorer: 0,
    };
    // SAFETY: `action` is a live public `struct sigaction` and `old` is NULL.
    unsafe { sigaction(SIGUSR1, &action, core::ptr::null_mut()) == 0 }
}

fn main() {
    let marker = 0u8;
    let expected_sp = &marker as *const u8 as usize;
    let text_high = b"K571: PASS".as_ptr() as usize;

    if !install() {
        println!("K571: fail install");
        std::process::exit(1);
    }
    // SAFETY: `raise` takes one scalar signal number.
    let raised = unsafe { raise(SIGUSR1) } == 0;
    AFTER_RAISE.store(1, Ordering::SeqCst);

    let ran = RAN.load(Ordering::SeqCst);
    let seen = SEEN_SIG.load(Ordering::SeqCst);
    let rip = INTERRUPTED_RIP.load(Ordering::SeqCst);
    let rsp = INTERRUPTED_RSP.load(Ordering::SeqCst);
    let after = AFTER_RAISE.load(Ordering::SeqCst);

    if !raised || ran != 1 || seen != SIGUSR1 {
        println!("K571: fail handler ran={ran} seen={seen}");
        std::process::exit(1);
    }
    println!("K571: handler ok sig={seen}");

    let text_ok = rip >= 0x400000 && rip < text_high;
    let stack_ok = rsp != 0 && rsp.abs_diff(expected_sp) < 1024 * 1024;
    if !text_ok || !stack_ok {
        println!("K571: fail ucontext rip=0x{rip:x} rsp=0x{rsp:x}");
        std::process::exit(1);
    }
    println!("K571: ucontext ok rip=0x{rip:x} rsp=0x{rsp:x}");

    if after != 1 {
        println!("K571: fail return after={after}");
        std::process::exit(1);
    }
    println!("K571: return ok");
    println!("K571: PASS");
}
