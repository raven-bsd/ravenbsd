// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S45 signal-fault fixture.
//!
//! Raises user faults through `SIG_SETUP`: an unmapped page (`SIGSEGV`/
//! `SEGV_MAPERR`), an `int3`/`brk` breakpoint (`SIGTRAP`), and an x86-64
//! divide error (`SIGFPE`). The entry receives the `FaultRecord` pointer in
//! its second argument, advances the saved PC past the faulting instruction
//! and returns through `SIGRETURN`. A child process with no registration dies
//! with the default SIGSEGV status.

#![no_std]
#![no_main]

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{self, FaultRecord, MContext};
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn_with};
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const CTX_OFF: usize = 0x100;
const SCENARIO_PAGE: u32 = 1;
const SCENARIO_DIVIDE: u32 = 2;
const SCENARIO_BREAK: u32 = 3;

static SCENARIO: AtomicU32 = AtomicU32::new(0);
static ENTRY_COUNT: AtomicU32 = AtomicU32::new(0);
static EXPECT_ADDR: AtomicU64 = AtomicU64::new(0);
static EXPECT_PC: AtomicU64 = AtomicU64::new(0);
static RESUME_PC: AtomicU64 = AtomicU64::new(0);

// The kernel enters the registered point with a 16-byte-aligned stack. The
// SysV ABI expects RSP % 16 == 8 at a Rust entry, so the x86 stub aligns and
// calls; the aarch64 ABI already requires a 16-byte-aligned SP.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global __s45_sigentry
    .type __s45_sigentry, @function
__s45_sigentry:
    and rsp, -16
    call s45_signal_body
    ud2

    /* Faulting sequences. The handler sets the saved PC to the `_after` label,
     * so the faulting instruction is skipped exactly once. */
    .global s45_page_fault_seq
    .type s45_page_fault_seq, @function
s45_page_fault_seq:
    mov rax, [rdi]
    .global s45_page_fault_after
s45_page_fault_after:
    ret

    .global s45_break_seq
    .type s45_break_seq, @function
s45_break_seq:
    int3
    .global s45_break_after
s45_break_after:
    ret

    .global s45_divide_seq
    .type s45_divide_seq, @function
s45_divide_seq:
    xor edx, edx
    mov eax, 1
    xor ecx, ecx
    .global s45_divide_fault
s45_divide_fault:
    div ecx
    .global s45_divide_after
s45_divide_after:
    ret
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global __s45_sigentry
    .type __s45_sigentry, %function
__s45_sigentry:
    bl s45_signal_body
    brk #0

    .global s45_page_fault_seq
    .type s45_page_fault_seq, %function
s45_page_fault_seq:
    ldr x0, [x0]
    .global s45_page_fault_after
s45_page_fault_after:
    ret

    .global s45_break_seq
    .type s45_break_seq, %function
s45_break_seq:
    brk #0
    .global s45_break_after
s45_break_after:
    ret
"#
);

unsafe extern "C" {
    fn __s45_sigentry();
    fn s45_page_fault_seq(address: usize);
    fn s45_page_fault_after();
    fn s45_break_seq();
    fn s45_break_after();
}

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn s45_divide_seq();
    fn s45_divide_fault();
    fn s45_divide_after();
}

fn signal_entry_address() -> usize {
    __s45_sigentry as *const () as usize
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
        Some("fault-child") => fault_child(),
        Some(_) => 2,
    }
}

fn controller() -> i32 {
    // Two pages: the aarch64 MContext plus the FaultRecord tail crosses one
    // 4 KiB page at CTX_OFF. The x86-64 context fits in the first page.
    let tls = match sys::map_anonymous(2 * PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    if set_tls(tls).is_err() {
        println!("signal-fault: tls failed");
        return 1;
    }
    // Install the TLS base on the hardware register before the first entry.
    let _ = sys::nanosleep(2_000_000);
    if sig_setup(signal_entry_address(), CTX_OFF, 0).is_err() {
        println!("signal-fault: setup failed");
        return 1;
    }
    println!("signal-fault: setup=0");

    let page = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    if sys::unmap(Handle::SELF_PROC, page, PAGE).is_err() {
        println!("signal-fault: unmap failed");
        return 1;
    }
    SCENARIO.store(SCENARIO_PAGE, Ordering::SeqCst);
    EXPECT_ADDR.store(page as u64, Ordering::SeqCst);
    EXPECT_PC.store(s45_page_fault_seq as *const () as u64, Ordering::SeqCst);
    RESUME_PC.store(s45_page_fault_after as *const () as u64, Ordering::SeqCst);
    unsafe {
        s45_page_fault_seq(page);
    }
    println!("signal-fault: page resumed=1");

    SCENARIO.store(SCENARIO_BREAK, Ordering::SeqCst);
    EXPECT_ADDR.store(0, Ordering::SeqCst);
    // x86 `int3` is a trap: the saved RIP already points after the opcode.
    // aarch64 `brk` reports the faulting instruction in ELR.
    #[cfg(target_arch = "x86_64")]
    EXPECT_PC.store(s45_break_after as *const () as u64, Ordering::SeqCst);
    #[cfg(target_arch = "aarch64")]
    EXPECT_PC.store(s45_break_seq as *const () as u64, Ordering::SeqCst);
    RESUME_PC.store(s45_break_after as *const () as u64, Ordering::SeqCst);
    unsafe {
        s45_break_seq();
    }
    println!("signal-fault: breakpoint resumed=1");

    #[cfg(target_arch = "x86_64")]
    {
        SCENARIO.store(SCENARIO_DIVIDE, Ordering::SeqCst);
        EXPECT_ADDR.store(0, Ordering::SeqCst);
        EXPECT_PC.store(s45_divide_fault as *const () as u64, Ordering::SeqCst);
        RESUME_PC.store(s45_divide_after as *const () as u64, Ordering::SeqCst);
        unsafe {
            s45_divide_seq();
        }
        println!("signal-fault: divide=fpe");
        println!("signal-fault: divide done=1");
    }
    #[cfg(target_arch = "aarch64")]
    {
        // AArch64 integer division by zero does not trap.
        println!("signal-fault: divide=skip");
        println!("signal-fault: divide done=1");
    }

    let image = match sys::map_initfs() {
        Ok(image) => image,
        Err(_) => {
            println!("signal-fault: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("signal-fault: initfs parse failed");
            return 1;
        }
    };
    let options = SpawnOptions {
        args: &["init", "fault-child"],
        env: &[],
    };
    let mut rt = RtSys;
    let child = match spawn_with(&initfs, "/init", &options, &mut rt) {
        Ok(child) => child,
        Err(_) => {
            println!("signal-fault: spawn failed");
            return 1;
        }
    };
    let mut code = 0i32;
    if rt.proc_wait(child, &mut code).is_err() {
        println!("signal-fault: wait failed");
        return 1;
    }
    println!("signal-fault: child status={code}");
    println!("signal-fault: done");
    0
}

fn fault_child() -> i32 {
    let page = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    if sys::unmap(Handle::SELF_PROC, page, PAGE).is_err() {
        println!("signal-fault: child unmap failed");
        return 1;
    }
    unsafe {
        s45_page_fault_seq(page);
    }
    println!("signal-fault: child returned");
    println!("signal-fault: child done");
    0
}

#[unsafe(no_mangle)]
extern "C" fn s45_signal_body(bits: usize, fault: *const FaultRecord) -> ! {
    let scenario = SCENARIO.load(Ordering::SeqCst);
    let count = ENTRY_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    let base = read_tls_base();
    let context_address = base.wrapping_add(CTX_OFF as u64);
    // SAFETY: the kernel registered this area through SIG_SETUP and wrote a
    // full MContext (and the FaultRecord tail) before entering; the TLS page
    // is mapped by the fixture.
    let context = unsafe { &mut *(context_address as *mut MContext) };
    if fault.is_null() {
        println!(
            "signal-fault: entry scenario={scenario} bits={bits} count={count} fault=0 ctx-size={}",
            size_of::<MContext>()
        );
    } else {
        // SAFETY: the kernel wrote one FaultRecord immediately after the
        // MContext in the registered user area.
        let record = unsafe { &*fault };
        let pc_match = record.pc as u64 == EXPECT_PC.load(Ordering::SeqCst);
        let addr_match = if scenario == SCENARIO_PAGE {
            record.addr as u64 == EXPECT_ADDR.load(Ordering::SeqCst)
        } else {
            record.addr == record.pc
        };
        println!(
            "signal-fault: entry scenario={} bits={} count={} fault=1 kind={} addr=0x{:x} pc=0x{:x} ctx-pc=0x{:x} ctx-size={} resume=0x{:x} pc-match={} addr-match={}",
            scenario,
            bits,
            count,
            record.kind,
            record.addr,
            record.pc,
            context.pc(),
            size_of::<MContext>(),
            RESUME_PC.load(Ordering::SeqCst),
            u8::from(pc_match),
            u8::from(addr_match)
        );
        match scenario {
            SCENARIO_PAGE => println!("signal-fault: page entry=1"),
            SCENARIO_DIVIDE => println!("signal-fault: divide entry=1"),
            SCENARIO_BREAK => println!("signal-fault: breakpoint entry=1"),
            _ => {}
        }
        context.set_pc(RESUME_PC.load(Ordering::SeqCst));
    }
    let raw = sigreturn();
    println!("signal-fault: sigreturn returned raw={raw}");
    loop {
        core::hint::spin_loop();
    }
}
