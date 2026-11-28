// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S779 plain `signal(2)` fault-handler fixture.
//!
//! Replays the NetBSD `t_mprotect.c` `mprotect_write` shape through the public ravenlibc ABI:
//! a page is mapped read/write with `mmap(NULL, page, PROT_READ|PROT_WRITE, MAP_ANON, -1, 0)`
//! (the donor passes `MAP_ANON` without a `MAP_PRIVATE`/`MAP_SHARED` type), written, made
//! read-only with `mprotect(2)`, and a child writes it again. The child installs
//! `signal(SIGSEGV, sighandler)` (`SA_RESTART`, no `SA_SIGINFO`); `sighandler` is the donor
//! `_exit(signo)`. The C19 kernel frame must enter the plain handler and `_exit(SIGSEGV)` must
//! terminate the child with status 11.
//!
//! The same image keeps the S776 coverage: an `SA_SIGINFO` handler observes
//! `SIGSEGV`/`SEGV_ACCERR` and the fault address, rewrites the saved RIP and resumes through
//! the kernel sigcode; a default-disposition child dies with status -11.
//!
//! `/init` stdout is not the serial console, so the fixture reports through the
//! `DEBUG_WRITE` syscall (`raven_abi::syscall::DEBUG_WRITE`, 0x03) like `raven-rt::println!`.
//!
//! The binary is `no_std` and links the real ravenlibc archive through `build.sh`; the C
//! `signal(2)`, `sigaction(2)`, `mmap(2)`, `mprotect(2)`, `fork(2)` and `waitpid(2)` calls go
//! through the same sysroot the product uses.

#![no_std]
#![no_main]

use core::ffi::{c_int, c_void};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use raven_abi::syscall::Syscall;
use raven_libc_abi::mman::{MAP_ANON, PROT_READ, PROT_WRITE};
use raven_libc_abi::signal::{SA_SIGINFO, SEGV_ACCERR, SIGSEGV};
use raven_libc_abi::structs::{Sigaction, SiginfoT};

const PAGE: usize = 4096;
const MAP_FAILED: usize = usize::MAX;
const SIG_ERR: usize = usize::MAX;
/// `SIG_DFL` is the null handler pointer.
const SIG_DFL: usize = 0;

/// The donor `mprotect_write` flags: `MAP_ANON` with no private/shared type.
#[cfg(not(feature = "private-map"))]
const MAP_FLAGS: c_int = MAP_ANON;
/// The explicit-private variant used to isolate plain-frame delivery from the map type.
#[cfg(feature = "private-map")]
const MAP_FLAGS: c_int = MAP_ANON | raven_libc_abi::mman::MAP_PRIVATE;

const _: () = assert!(core::mem::size_of::<Sigaction>() == 152);

static SAVED_SIGNO: AtomicI32 = AtomicI32::new(0);
static SAVED_CODE: AtomicI32 = AtomicI32::new(0);
static SAVED_ADDR: AtomicUsize = AtomicUsize::new(0);
static RESUME_PC: AtomicUsize = AtomicUsize::new(0);

/// System V amd64 `mcontext_t` as the kernel writes it into `ucontext_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct MContext {
    gregs: [u64; 23],
    fpregs: u64,
    reserved: [u64; 8],
}

/// System V amd64 `ucontext_t` (936 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct UContext {
    flags: u64,
    link: u64,
    stack: [u64; 3],
    mcontext: MContext,
    sigmask: [u64; 16],
    fpregs: [u64; 64],
}

const _: () = assert!(core::mem::size_of::<UContext>() == 936);
const _: () = assert!(core::mem::offset_of!(UContext, mcontext) == 40);
/// `REG_RIP` index in `gregs`.
const REG_PC: usize = 16;

impl UContext {
    /// Resume at `pc` when the kernel restores this context.
    fn set_pc(&mut self, pc: usize) {
        if let Some(slot) = self.mcontext.gregs.get_mut(REG_PC) {
            *slot = pc as u64;
        }
    }
}

core::arch::global_asm!(
    r#"
    .section .text
    .global s779_fault_seq
    .type s779_fault_seq, @function
s779_fault_seq:
    mov byte ptr [rdi], 1
    .global s779_fault_after
s779_fault_after:
    ret
"#
);

unsafe extern "C" {
    fn s779_fault_seq(address: usize);
    fn s779_fault_after();

    fn mmap(
        address: *mut c_void,
        length: usize,
        protection: c_int,
        flags: c_int,
        fd: c_int,
        offset: i64,
    ) -> *mut c_void;
    fn mprotect(address: *mut c_void, length: usize, protection: c_int) -> c_int;
    fn sigaction(signo: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn signal(signo: c_int, handler: usize) -> usize;
    fn fork() -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn _exit(status: c_int) -> !;
    fn __errno_location() -> *mut c_int;
}

fn errno() -> c_int {
    // SAFETY: `__errno_location` returns this thread's live errno cell.
    unsafe { *__errno_location() }
}

/// Write to the kernel debug console: `/init` has no serial stdout.
fn console_write(text: &[u8]) {
    // SAFETY: DEBUG_WRITE reads `text.len()` bytes from the slice's valid pointer.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") Syscall::DEBUG_WRITE as usize => _,
            in("rdi") text.as_ptr(),
            in("rsi") text.len(),
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
}

fn emit(text: &[u8]) {
    console_write(text);
}

fn emit_i32(prefix: &[u8], value: c_int, suffix: &[u8]) {
    let mut digits = [0u8; 12];
    let mut length = 0usize;
    let negative = value < 0;
    let mut magnitude = if negative {
        (value as u32).wrapping_neg()
    } else {
        value as u32
    };
    loop {
        digits[length] = b'0' + (magnitude % 10) as u8;
        length += 1;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    emit(prefix);
    if negative {
        emit(b"-");
    }
    while length > 0 {
        length -= 1;
        console_write(&digits[length..length + 1]);
    }
    emit(suffix);
}

fn emit_hex(prefix: &[u8], value: usize, suffix: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut digits = [0u8; 2 * core::mem::size_of::<usize>()];
    let mut length = 0usize;
    let mut remaining = value;
    loop {
        digits[length] = HEX[remaining & 0xf];
        length += 1;
        remaining >>= 4;
        if remaining == 0 {
            break;
        }
    }
    emit(prefix);
    while length > 0 {
        length -= 1;
        console_write(&digits[length..length + 1]);
    }
    emit(suffix);
}

fn fault_write(page: usize) {
    // SAFETY: the caller mapped one page and dropped write permission; the fault is the test.
    unsafe { s779_fault_seq(page) };
}

fn map_read_only() -> Option<usize> {
    // SAFETY: the arguments are scalar and the flags are the exact donor shape under test.
    let mapped = unsafe { mmap(null_mut(), PAGE, PROT_READ | PROT_WRITE, MAP_FLAGS, -1, 0) };
    if mapped as usize == MAP_FAILED {
        emit_i32(b"signal-fault: donor-mmap failed errno=", errno(), b"\n");
        return None;
    }
    let page = mapped as usize;
    // SAFETY: the mapping has PAGE writable bytes.
    unsafe { core::ptr::write_bytes(page as *mut u8, b'A', PAGE) };
    // SAFETY: the mapping and length are valid; read-only is the `t_mprotect` transition.
    if unsafe { mprotect(mapped, PAGE, PROT_READ) } != 0 {
        emit(b"signal-fault: mprotect failed\n");
        return None;
    }
    Some(page)
}

fn install_info_handler() -> bool {
    let action = Sigaction {
        sa_handler: info_handler as *const () as *mut c_void,
        sa_flags: SA_SIGINFO,
        ..Sigaction::ZERO
    };
    // SAFETY: `action` is live for the call and `sigaction` retains no pointer.
    unsafe { sigaction(SIGSEGV, &action, null_mut()) == 0 }
}

fn wifexited(status: c_int) -> bool {
    status & 0x7f == 0
}

fn wexitstatus(status: c_int) -> c_int {
    (status >> 8) & 0xff
}

fn wifsignaled(status: c_int) -> bool {
    let signal = status & 0x7f;
    signal != 0 && signal != 0x7f
}

fn wtermsig(status: c_int) -> c_int {
    status & 0x7f
}

fn run_caught_child(page: usize) -> bool {
    // SAFETY: `fork` takes no arguments; the child path never returns to the parent.
    let pid = unsafe { fork() };
    if pid < 0 {
        return false;
    }
    if pid == 0 {
        // SAFETY: the plain `signal(2)` ABI takes the handler address as one word.
        if unsafe { signal(SIGSEGV, exit_handler as *const () as usize) } == SIG_ERR {
            unsafe { _exit(101) };
        }
        fault_write(page);
        // SAFETY: the fault must never return for the plain `_exit` handler.
        unsafe { _exit(102) };
    }
    let mut status = 0;
    // SAFETY: `status` is a live `int` for the whole call.
    if unsafe { waitpid(pid, &mut status, 0) } != pid || !wifexited(status) {
        return false;
    }
    let code = wexitstatus(status);
    emit_i32(b"signal-fault: caught status=", code, b"\n");
    code == SIGSEGV
}

fn run_default_child(page: usize) -> bool {
    // SAFETY: `fork` takes no arguments; the child path never returns to the parent.
    let pid = unsafe { fork() };
    if pid < 0 {
        return false;
    }
    if pid == 0 {
        // SAFETY: the plain `signal(2)` ABI takes the handler address as one word.
        if unsafe { signal(SIGSEGV, SIG_DFL) } == SIG_ERR {
            unsafe { _exit(103) };
        }
        fault_write(page);
        // SAFETY: the default disposition must kill the child at the fault.
        unsafe { _exit(104) };
    }
    let mut status = 0;
    // SAFETY: `status` is a live `int` for the whole call.
    if unsafe { waitpid(pid, &mut status, 0) } != pid || !wifsignaled(status) {
        return false;
    }
    let signal = wtermsig(status);
    emit_i32(b"signal-fault: default status=-", signal, b"\n");
    signal == SIGSEGV
}

/// The C `main` signature `__raven_libc_start` calls through `crt1.o`.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int {
    let page = match map_read_only() {
        Some(page) => page,
        None => return 1,
    };
    if !install_info_handler() {
        emit(b"signal-fault: sigaction failed\n");
        return 1;
    }
    RESUME_PC.store(s779_fault_after as *const () as usize, Ordering::SeqCst);
    fault_write(page);
    if SAVED_SIGNO.load(Ordering::SeqCst) != SIGSEGV {
        emit(b"signal-fault: page handler=0\n");
        return 1;
    }
    let code = SAVED_CODE.load(Ordering::SeqCst);
    let address = SAVED_ADDR.load(Ordering::SeqCst);
    emit(b"signal-fault: page handler=1");
    emit_i32(b" code=", code, b"");
    emit_hex(b" addr=0x", address, b"");
    let matches = if address == page && code == SEGV_ACCERR {
        1
    } else {
        0
    };
    emit_i32(b" addr-match=", matches, b"\n");
    emit(b"signal-fault: page resumed=1\n");
    if !run_caught_child(page) || !run_default_child(page) {
        return 1;
    }
    emit(b"signal-fault: done\n");
    0
}

extern "C" fn info_handler(signo: c_int, info: *const SiginfoT, context: *mut UContext) {
    if info.is_null() || context.is_null() {
        return;
    }
    // SAFETY: the kernel passes a siginfo and ucontext it just wrote inside the handler frame.
    let (code, address) = unsafe {
        (
            (*info).si_code,
            (*info).__si_fields.first().copied().unwrap_or(0) as usize,
        )
    };
    SAVED_SIGNO.store(signo, Ordering::SeqCst);
    SAVED_CODE.store(code, Ordering::SeqCst);
    SAVED_ADDR.store(address, Ordering::SeqCst);
    // SAFETY: the kernel wrote the full ucontext_t; only `gregs[REG_RIP]` is modified.
    unsafe { (*context).set_pc(RESUME_PC.load(Ordering::SeqCst)) };
}

/// NetBSD `t_mprotect.c` `sighandler`: exit with the signal number.
extern "C" fn exit_handler(signo: c_int) {
    // SAFETY: `_exit` never returns.
    unsafe { _exit(signo) };
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // SAFETY: `_exit` never returns.
    unsafe { _exit(127) };
}
