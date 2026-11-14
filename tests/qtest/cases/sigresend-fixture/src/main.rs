// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S46 SIGRESEND fixture.
//!
//! A kick enters the signal entry; the first entry saves the interrupted
//! context and calls `SIGRESEND` with a new mask. The kernel re-enters the
//! entry (nested) with the mask in the third argument; the second entry
//! records the mask/order and returns through `SIGRETURN` to the first entry.
//! The first entry restores the original context and returns to the
//! interrupted code. A second thread without `SIG_SETUP` gets `EINVAL`.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{self, MContext};
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const CHECK: u32 = 1 << 0;
const PAGE: usize = 4096;
const CTX_OFF: usize = 0x100;
const RESEND_MASK: u64 = 0x5555_aaaa_1234_5678;

static STAGE: AtomicU32 = AtomicU32::new(0);
static ORDER: AtomicU32 = AtomicU32::new(0);
static EXPECT_MASK: AtomicU64 = AtomicU64::new(0);
static NO_SETUP_DONE: AtomicBool = AtomicBool::new(false);

// Original interrupted context saved by the first entry; the second entry's
// SIGRETURN returns to the first entry, which then restores this context.
static mut SAVED_ORIGINAL: MContext = MContext::zero();

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global __s46_sigentry
    .type __s46_sigentry, @function
__s46_sigentry:
    and rsp, -16
    call s46_signal_body
    ud2
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global __s46_sigentry
    .type __s46_sigentry, %function
__s46_sigentry:
    bl s46_signal_body
    brk #0
"#
);

unsafe extern "C" {
    fn __s46_sigentry();
}

fn signal_entry_address() -> usize {
    __s46_sigentry as *const () as usize
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

fn sigresend(sig: u32, info: usize, mask: u64) -> isize {
    unsafe {
        syscall6(
            signal::SIGRESEND,
            sig as usize,
            info,
            mask as usize,
            0,
            0,
            0,
        )
    }
}

fn sigreturn() -> isize {
    unsafe { syscall6(signal::SIGRETURN, 0, 0, 0, 0, 0, 0) }
}

fn kick_self() -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            raven_abi::syscall::THREAD_KICK,
            Handle::NONE.0 as usize,
            CHECK as usize,
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

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        None | Some("init") => controller(),
        Some(_) => 2,
    }
}

fn controller() -> i32 {
    let tls = match sys::map_anonymous(2 * PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    if set_tls(tls).is_err() {
        println!("sigresend: tls failed");
        return 1;
    }
    let _ = sys::nanosleep(2_000_000);
    if sig_setup(signal_entry_address(), CTX_OFF, 0).is_err() {
        println!("sigresend: setup failed");
        return 1;
    }
    println!("sigresend: setup=0");

    let no_setup_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let _ = sys::thread_create(
        Handle::SELF_PROC,
        resend_child as *const () as usize,
        no_setup_stack + 2 * PAGE,
        0,
    );
    let _ = sys::nanosleep(200_000_000);
    if !NO_SETUP_DONE.load(Ordering::SeqCst) {
        println!("sigresend: no-setup thread did not finish");
        return 1;
    }

    EXPECT_MASK.store(RESEND_MASK, Ordering::SeqCst);
    STAGE.store(0, Ordering::SeqCst);
    ORDER.store(0, Ordering::SeqCst);
    if kick_self().is_err() {
        println!("sigresend: kick failed");
        return 1;
    }
    if ORDER.load(Ordering::SeqCst) != 2 {
        println!("sigresend: order not recorded");
        return 1;
    }
    println!("sigresend: original resumed=1");
    println!("sigresend: done");
    0
}

extern "C" fn resend_child(_arg: usize) -> ! {
    let raw = sigresend(0, 0, 0x55);
    if raw == Errno::EINVAL.to_ret() {
        println!("sigresend: no-setup=EINVAL");
    } else {
        println!("sigresend: no-setup unexpected raw={raw}");
    }
    NO_SETUP_DONE.store(true, Ordering::SeqCst);
    sys::thread_exit(0)
}

#[unsafe(no_mangle)]
extern "C" fn s46_signal_body(bits: usize, fault: *const u8, mask: u64) -> ! {
    let scenario = STAGE.load(Ordering::SeqCst);
    if !fault.is_null() {
        println!("sigresend: unexpected fault pointer");
    }
    let base = read_tls_base();
    let context_address = base.wrapping_add(CTX_OFF as u64);
    // SAFETY: the kernel registered this area through SIG_SETUP and wrote a
    // full MContext there before entering; the TLS mapping stays alive.
    let context = unsafe { &mut *(context_address as *mut MContext) };
    match scenario {
        0 => {
            // First entry from the kick: preserve the interrupted context.
            // SAFETY: this is the only thread touching the static; the copy
            // covers one fully initialised MContext.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    context as *const MContext,
                    core::ptr::addr_of_mut!(SAVED_ORIGINAL),
                    1,
                );
            }
            STAGE.store(1, Ordering::SeqCst);
            println!("sigresend: first entry bits={bits} mask=0x{mask:x}");
            let raw = sigresend(0, 0, RESEND_MASK);
            println!(
                "sigresend: first resumed order={} raw={raw}",
                ORDER.load(Ordering::SeqCst)
            );
            // Return to the interrupted code, not to the SIGRESEND call site.
            // SAFETY: the static holds the context saved at first entry, and
            // the area is the kernel's registered MContext.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    core::ptr::addr_of!(SAVED_ORIGINAL),
                    context as *mut MContext,
                    1,
                );
            }
            let raw = sigreturn();
            println!("sigresend: first sigreturn returned raw={raw}");
        }
        1 => {
            let expected = EXPECT_MASK.load(Ordering::SeqCst);
            if mask == expected {
                println!("sigresend: second entry bits={bits} mask=0x{mask:x} order=2 match=1");
            } else {
                println!(
                    "sigresend: second entry bits={bits} mask=0x{mask:x} order=2 match=0 expected=0x{expected:x}"
                );
            }
            ORDER.store(2, Ordering::SeqCst);
            STAGE.store(2, Ordering::SeqCst);
            let raw = sigreturn();
            println!("sigresend: second sigreturn returned raw={raw}");
        }
        _ => println!("sigresend: unexpected stage={scenario}"),
    }
    loop {
        core::hint::spin_loop();
    }
}
