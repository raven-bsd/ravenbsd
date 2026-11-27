// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S776 signal-fault fixture.
//!
//! C19 synchronous-fault delivery: the fixture installs a `SIGSEGV` disposition with the
//! private 40-byte `SIGACTION` request (never `SIG_SETUP`), writes a read-only page, and checks
//! that the kernel-built frame runs the handler with `si_signo`/`si_code`/`si_addr` for the
//! permission fault. The handler rewrites the saved PC in the `ucontext_t` and returns through
//! the kernel sigcode, so `SIGRETURN` must resume the interrupted execution.
//!
//! Two children exercise the default paths: one installs a handler that exits with the signal
//! number (the NetBSD `t_mprotect` shape) and must report status 11; the other keeps the default
//! disposition and must die with the SIGSEGV status -11.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn_with};
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const SIGSEGV: i32 = 11;
/// `SEGV_ACCERR` (FreeBSD `sys/sys/signal.h`; `raven-libc-abi` generated/signal.rs).
const SEGV_ACCERR: u64 = 2;

static SAVED_SIG: AtomicU64 = AtomicU64::new(0);
static SAVED_CODE: AtomicU64 = AtomicU64::new(0);
static SAVED_ADDR: AtomicU64 = AtomicU64::new(0);
static RESUME_PC: AtomicU64 = AtomicU64::new(0);

/// The Raven private 40-byte `SIGACTION` request (`kernel/src/syscall/thread.rs:UserSigAction`).
#[repr(C)]
#[derive(Clone, Copy)]
struct KernelSigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

const _: () = assert!(core::mem::size_of::<KernelSigAction>() == 40);

#[cfg(target_arch = "x86_64")]
mod frame {
    /// System V amd64 `siginfo_t`; the union starts after the 12-byte header plus padding.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SigInfo {
        signo: i32,
        errno: i32,
        code: i32,
        _pad: i32,
        pub fields: [u8; 112],
    }

    impl SigInfo {
        /// `si_addr` is the first word of the union (`_sigfault.si_addr`).
        pub fn addr(&self) -> u64 {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&self.fields[..8]);
            u64::from_le_bytes(bytes)
        }

        /// `si_code`.
        pub fn code(&self) -> i32 {
            self.code
        }

        /// `si_signo`.
        pub fn signo(&self) -> i32 {
            self.signo
        }
    }

    /// System V amd64 `stack_t`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Stack {
        sp: u64,
        flags: i32,
        _pad: i32,
        size: u64,
    }

    /// C36 `mcontext_t`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct MContext {
        pub gregs: [u64; 23],
        fpregs: u64,
        reserved: [u64; 8],
    }

    /// System V amd64 `ucontext_t` (936 bytes).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct UContext {
        flags: u64,
        link: u64,
        stack: Stack,
        pub mcontext: MContext,
        sigmask: [u64; 16],
        fpregs: [u64; 64],
    }

    const _: () = assert!(core::mem::size_of::<UContext>() == 936);
    const _: () = assert!(core::mem::offset_of!(UContext, mcontext) == 40);

    /// `REG_RIP` index in `gregs`.
    pub const REG_PC: usize = 16;
}

#[cfg(target_arch = "aarch64")]
mod frame {
    /// C19 aarch64 `siginfo_t` (`S160` layout).
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct SigInfo {
        signo: i32,
        errno: i32,
        code: i32,
        _pid: i32,
        _uid: u32,
        _status: i32,
        addr: u64,
        _value: u64,
        _reason: [u8; 0x28],
    }

    impl SigInfo {
        /// `si_addr`.
        pub fn addr(&self) -> u64 {
            self.addr
        }

        /// `si_code`.
        pub fn code(&self) -> i32 {
            self.code
        }

        /// `si_signo`.
        pub fn signo(&self) -> i32 {
            self.signo
        }
    }

    /// aarch64 `stack_t`.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Stack {
        sp: u64,
        size: u64,
        flags: u32,
        _spare: u32,
    }

    /// NetBSD aarch64 `__fregset_t`.
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct Fpregset {
        q: [[u8; 16]; 32],
        fpcr: u32,
        fpsr: u32,
    }

    /// NetBSD aarch64 `mcontext_t`.
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct MContext {
        pub gregs: [u64; 35],
        fregs: Fpregset,
        spare: [u64; 8],
    }

    /// C19 aarch64 `ucontext_t`.
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    pub struct UContext {
        sigmask: [u32; 4],
        pub mcontext: MContext,
        link: u64,
        stack: Stack,
        flags: u32,
        spare: [u32; 4],
    }

    const _: () = assert!(core::mem::size_of::<UContext>() == 0x3c0);

    /// `REG_ELR` index in `__gregs`.
    pub const REG_PC: usize = 32;
}

impl frame::UContext {
    /// Resume at `pc` when `SIGRETURN` restores this context.
    fn set_pc(&mut self, pc: u64) {
        if let Some(slot) = self.mcontext.gregs.get_mut(frame::REG_PC) {
            *slot = pc;
        }
    }
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s776_fault_seq
    .type s776_fault_seq, @function
s776_fault_seq:
    mov byte ptr [rdi], 1
    .global s776_fault_after
s776_fault_after:
    ret
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s776_fault_seq
    .type s776_fault_seq, %function
s776_fault_seq:
    strb w0, [x0]
    .global s776_fault_after
s776_fault_after:
    ret
"#
);

unsafe extern "C" {
    fn s776_fault_seq(address: usize);
    fn s776_fault_after();
}

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

type Handler = extern "C" fn(i32, *const frame::SigInfo, *mut frame::UContext);

fn install(signo: i32, handler: Handler) -> Result<(), Errno> {
    let action = KernelSigAction {
        handler: handler as *const () as u64,
        mask: [0; 2],
        flags: 0,
        _pad: 0,
        restorer: 0,
    };
    result(unsafe {
        sys::syscall6(
            raven_abi::syscall::SIGACTION,
            signo as usize,
            &action as *const KernelSigAction as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

/// Drop write permission on one page (the `t_mprotect` read-only transition).
fn protect_read_only(page: usize) -> Result<(), Errno> {
    result(unsafe {
        sys::syscall4(
            raven_abi::syscall::Syscall::PROTECT as usize,
            Handle::SELF_PROC.0 as usize,
            page,
            PAGE,
            prot::READ,
        )
    })
    .map(|_| ())
}

fn fault_write(page: usize) {
    // SAFETY: the caller mapped one page and dropped write permission; the fault is the test.
    unsafe { s776_fault_seq(page) };
}

fn main(args: raven_rt::Args) -> i32 {
    match args.get(1) {
        None | Some("init") => controller(),
        Some("caught-child") => caught_child(),
        Some("default-child") => default_child(),
        Some(_) => 2,
    }
}

fn controller() -> i32 {
    let page = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    // SAFETY: the mapping has PAGE writable bytes.
    unsafe { core::ptr::write_volatile(page as *mut u8, 0x41) };
    if protect_read_only(page).is_err() {
        println!("signal-fault: protect failed");
        return 1;
    }
    if install(SIGSEGV, fault_handler).is_err() {
        println!("signal-fault: sigaction failed");
        return 1;
    }
    RESUME_PC.store(s776_fault_after as *const () as u64, Ordering::SeqCst);
    fault_write(page);
    if SAVED_SIG.load(Ordering::SeqCst) != u64::from(SIGSEGV as u32) {
        println!("signal-fault: page handler=0");
        return 1;
    }
    let code = SAVED_CODE.load(Ordering::SeqCst);
    let addr = SAVED_ADDR.load(Ordering::SeqCst);
    println!(
        "signal-fault: page handler=1 code={code} addr=0x{addr:x} addr-match={}",
        u8::from(addr == page as u64 && code == SEGV_ACCERR)
    );
    println!("signal-fault: page resumed=1");

    if let Err(code) = run_child("caught-child") {
        println!("signal-fault: caught child failed={code}");
        return 1;
    }
    if let Err(code) = run_child("default-child") {
        println!("signal-fault: default child failed={code}");
        return 1;
    }
    println!("signal-fault: done");
    0
}

fn run_child(argument: &str) -> Result<i32, i32> {
    let image = sys::map_initfs().map_err(|_| 1)?;
    let initfs = Initfs::parse(image).map_err(|_| 2)?;
    let options = SpawnOptions {
        args: &["init", argument],
        env: &[],
    };
    let mut rt = RtSys;
    let child = spawn_with(&initfs, "/init", &options, &mut rt).map_err(|_| 3)?;
    let mut code = 0i32;
    rt.proc_wait(child, &mut code).map_err(|_| 4)?;
    if argument == "caught-child" {
        println!("signal-fault: caught status={code}");
    } else {
        println!("signal-fault: default status={code}");
    }
    Ok(code)
}

fn caught_child() -> i32 {
    let page = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    // SAFETY: the mapping has PAGE writable bytes.
    unsafe { core::ptr::write_volatile(page as *mut u8, 0x41) };
    if protect_read_only(page).is_err() {
        println!("signal-fault: child protect failed");
        return 1;
    }
    if install(SIGSEGV, exit_handler).is_err() {
        println!("signal-fault: child sigaction failed");
        return 1;
    }
    fault_write(page);
    println!("signal-fault: caught child survived");
    0
}

fn default_child() -> i32 {
    let page = match sys::map_anonymous(PAGE) {
        Ok(page) => page,
        Err(_) => return 1,
    };
    // SAFETY: the mapping has PAGE writable bytes.
    unsafe { core::ptr::write_volatile(page as *mut u8, 0x41) };
    if protect_read_only(page).is_err() {
        println!("signal-fault: default protect failed");
        return 1;
    }
    fault_write(page);
    println!("signal-fault: default child survived");
    0
}

extern "C" fn fault_handler(
    _sig: i32,
    info: *const frame::SigInfo,
    context: *mut frame::UContext,
) {
    if info.is_null() || context.is_null() {
        return;
    }
    // SAFETY: the kernel passes the siginfo and ucontext it just wrote inside the handler frame.
    let (signo, code, addr) = unsafe { ((*info).signo(), (*info).code(), (*info).addr()) };
    SAVED_SIG.store(u64::from(signo as u32), Ordering::SeqCst);
    SAVED_CODE.store(u64::from(code as u32), Ordering::SeqCst);
    SAVED_ADDR.store(addr, Ordering::SeqCst);
    // SAFETY: the kernel wrote the full ucontext_t; only `gregs` is modified.
    unsafe { (*context).set_pc(RESUME_PC.load(Ordering::SeqCst)) };
}

extern "C" fn exit_handler(sig: i32, _info: *const frame::SigInfo, _context: *mut frame::UContext) {
    // NetBSD `t_mprotect` `sighandler`: exit with the signal number.
    sys::exit(sig);
}
