// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S710 public-libc `sigaction`/`sigprocmask` boundary fixture.
//!
//! This is a Rust libc acceptance program in the `tests/libc/std-link`/`tests/libc/direct`
//! pattern: it calls the installed public libc entry points and links against
//! `build/<arch>/sysroot/usr/lib/libc.a`. It never crosses the private 40-byte kernel request
//! itself; libc performs that marshal. The public C `struct sigaction` is the 152-byte
//! FreeBSD layout (`libc/ravenlibc/crates/raven-libc-abi/src/generated/structs.rs`).
//!
//! The new and old public objects sit in the last struct slot before an inaccessible guard
//! page. A second install runs from a canary-filled page, so a libc or kernel write wider than
//! the public object shows up as damaged canary bytes. The installed flags and mask are read
//! back through libc, and `sigprocmask` is exercised through the same public mask type.

#![no_std]
#![no_main]

use core::ffi::{c_int, c_void};
use core::ptr;
use core::sync::atomic::{AtomicI32, Ordering};

const PAGE: usize = 4096;
const PUBLIC_SIGACTION: usize = 152;
const CANARY: u8 = 0x5a;

// `bits/signum.h` and `bits/abi_mman.h` from the installed sysroot.
const SIGUSR1: c_int = 10;
const SIGUSR2: c_int = 12;
const SA_ONSTACK: c_int = 0x0800_0000;
const SA_SIGINFO: c_int = 4;
const SIG_BLOCK: c_int = 0;
const SIG_UNBLOCK: c_int = 1;
const PROT_NONE: c_int = 0x0;
const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const MAP_PRIVATE: c_int = 0x2;
const MAP_ANON: c_int = 0x20;
const MAP_FAILED: usize = usize::MAX;

// `crates/raven-abi/src/syscall.rs`: DEBUG_WRITE = 0x03. Only the log sink; every signal call
// below is a real libc call.
const DEBUG_WRITE: usize = 3;

/// The public 128-byte `sigset_t` (`raven-libc-signal/src/sigset.rs`).
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigset {
    bytes: [u64; 16],
}

impl Sigset {
    const ZERO: Self = Self { bytes: [0; 16] };
}

/// The public 152-byte C `struct sigaction` (`generated/structs.rs`): handler 0, mask 8,
/// flags 136, restorer 144.
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigaction {
    sa_handler: usize,
    sa_mask: Sigset,
    sa_flags: c_int,
    _pad: c_int,
    sa_restorer: usize,
}

const _: () = assert!(core::mem::size_of::<Sigset>() == 128);
const _: () = assert!(core::mem::size_of::<Sigaction>() == PUBLIC_SIGACTION);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_handler) == 0);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_mask) == 8);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_flags) == 136);
const _: () = assert!(core::mem::offset_of!(Sigaction, sa_restorer) == 144);

unsafe extern "C" {
    fn sigaction(sig: c_int, act: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn sigprocmask(how: c_int, set: *const Sigset, old: *mut Sigset) -> c_int;
    fn sigemptyset(set: *mut Sigset) -> c_int;
    fn sigaddset(set: *mut Sigset, sig: c_int) -> c_int;
    fn sigismember(set: *const Sigset, sig: c_int) -> c_int;
    fn mmap(
        addr: *mut c_void,
        len: usize,
        protection: c_int,
        flags: c_int,
        fd: c_int,
        offset: i64,
    ) -> *mut c_void;
    fn mprotect(addr: *mut c_void, len: usize, protection: c_int) -> c_int;
    fn munmap(addr: *mut c_void, len: usize) -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn fork() -> c_int;
    fn read(fd: c_int, buf: *mut u8, count: usize) -> isize;
    fn write(fd: c_int, buf: *const u8, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
    fn getpid() -> c_int;
    fn getppid() -> c_int;
    fn _exit(code: c_int) -> !;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn __errno_location() -> *mut c_int;
    fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> c_int;
}

/// `struct timespec` for the child's bounded delay after the parent announces its read.
#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

extern "C" fn handler(_signal: c_int) {}

fn handler_address() -> usize {
    handler as *const () as usize
}

fn debug_write(bytes: &[u8]) {
    // SAFETY: DEBUG_WRITE reads `bytes.len()` bytes from a live slice and returns a byte count.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "syscall",
            inlateout("rax") DEBUG_WRITE => _,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "svc #0",
            in("x8") DEBUG_WRITE,
            inlateout("x0") bytes.as_ptr() => _,
            in("x1") bytes.len(),
            options(nostack),
        );
    }
}

fn write_hex(mut value: u32, out: &mut [u8; 8]) -> usize {
    let mut count = 0;
    if value == 0 {
        out[count] = b'0';
        count += 1;
    }
    while value != 0 && count < out.len() {
        let digit = (value & 0xf) as u8;
        out[count] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        value >>= 4;
        count += 1;
    }
    count
}

/// Emit `signal: boundary <name> ok flags=0x<hex> mask=1`.
fn row_ok(name: &[u8], flags: c_int, mask_member: bool) {
    let mut line = [0u8; 96];
    let mut at = 0;
    for byte in b"signal: boundary " {
        line[at] = *byte;
        at += 1;
    }
    for byte in name {
        line[at] = *byte;
        at += 1;
    }
    for byte in b" ok flags=0x" {
        line[at] = *byte;
        at += 1;
    }
    let mut digits = [0u8; 8];
    let count = write_hex(flags as u32, &mut digits);
    for index in (0..count).rev() {
        line[at] = digits[index];
        at += 1;
    }
    for byte in b" mask=" {
        line[at] = *byte;
        at += 1;
    }
    line[at] = if mask_member { b'1' } else { b'0' };
    at += 1;
    line[at] = b'\n';
    at += 1;
    debug_write(&line[..at]);
}

/// Emit `signal: boundary procmask ok mask=<member>`.
fn procmask_ok(mask_member: bool) {
    let mut line = [0u8; 48];
    let mut at = 0;
    for byte in b"signal: boundary procmask ok mask=" {
        line[at] = *byte;
        at += 1;
    }
    line[at] = if mask_member { b'1' } else { b'0' };
    at += 1;
    line[at] = b'\n';
    at += 1;
    debug_write(&line[..at]);
}

/// Emit `signal: boundary-fail <name>`.
fn row_fail(name: &[u8]) {
    let mut line = [0u8; 64];
    let mut at = 0;
    for byte in b"signal: boundary-fail " {
        line[at] = *byte;
        at += 1;
    }
    for byte in name {
        line[at] = *byte;
        at += 1;
    }
    line[at] = b'\n';
    at += 1;
    debug_write(&line[..at]);
}

/// Map `pages` zeroed writable pages.
fn map(pages: usize) -> Option<usize> {
    // SAFETY: the libc mmap contract; no file descriptor is used with MAP_ANON.
    let address = unsafe {
        mmap(
            ptr::null_mut(),
            pages * PAGE,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0,
        )
    } as usize;
    if address == MAP_FAILED {
        None
    } else {
        Some(address)
    }
}

fn unmap(address: usize, pages: usize) {
    // SAFETY: `address`/`pages` name a mapping returned by `map`.
    let _ = unsafe { munmap(address as *mut c_void, pages * PAGE) };
}

fn protect_none(address: usize) -> bool {
    // SAFETY: `address` is a page-aligned page inside a live mapping.
    unsafe { mprotect(address as *mut c_void, PAGE, PROT_NONE) == 0 }
}

fn fill(address: usize, byte: u8, len: usize) {
    // SAFETY: the caller passes a live writable range.
    unsafe { ptr::write_bytes(address as *mut u8, byte, len) };
}

fn canaries_intact(base: usize, holes: &[(usize, usize)]) -> bool {
    for offset in 0..PAGE {
        if holes
            .iter()
            .any(|(start, end)| offset >= *start && offset < *end)
        {
            continue;
        }
        // SAFETY: `offset` is inside the live page mapped by the caller.
        if unsafe { ptr::read((base + offset) as *const u8) } != CANARY {
            return false;
        }
    }
    true
}

/// Install/query with both public objects in the last slot before a guard page.
fn guard_row() -> bool {
    let region = match map(4) {
        Some(region) => region,
        None => {
            row_fail(b"mmap-region");
            return false;
        }
    };
    if !protect_none(region + PAGE) || !protect_none(region + 3 * PAGE) {
        unmap(region, 4);
        row_fail(b"mprotect-region");
        return false;
    }

    let act_address = region + PAGE - PUBLIC_SIGACTION;
    let old_address = region + 3 * PAGE - PUBLIC_SIGACTION;
    // SAFETY: both addresses are 8-aligned writable struct slots inside the mapping.
    let act = unsafe { &mut *(act_address as *mut Sigaction) };
    let old = unsafe { &mut *(old_address as *mut Sigaction) };
    *act = Sigaction {
        sa_handler: handler_address(),
        sa_mask: Sigset::ZERO,
        sa_flags: SA_SIGINFO | SA_ONSTACK,
        _pad: 0,
        sa_restorer: 0,
    };
    *old = Sigaction {
        sa_handler: 0,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: `act.sa_mask` is a live writable public sigset_t.
    let added = unsafe { sigaddset(&mut act.sa_mask, SIGUSR2) };

    let mut ok = added == 0;
    if ok {
        // SAFETY: both objects are live and the guard pages bound any wider access.
        ok = unsafe { sigaction(SIGUSR1, act, ptr::null_mut()) } == 0;
    }
    if ok {
        // SAFETY: `old` is a live writable public sigaction.
        ok = unsafe { sigaction(SIGUSR1, ptr::null(), old) } == 0;
    }
    if ok {
        // SAFETY: `old.sa_mask` is a live readable public sigset_t.
        let member = unsafe { sigismember(&old.sa_mask, SIGUSR2) } == 1;
        ok = old.sa_handler == handler_address()
            && old.sa_flags & (SA_SIGINFO | SA_ONSTACK) == SA_SIGINFO | SA_ONSTACK
            && member;
        if ok {
            row_ok(b"guard", old.sa_flags, member);
        }
    }
    unmap(region, 4);
    if !ok {
        row_fail(b"guard");
    }
    ok
}

/// Install/query from a canary-filled page and prove no byte outside the public objects moved.
fn canary_row() -> bool {
    let page = match map(1) {
        Some(page) => page,
        None => {
            row_fail(b"mmap-canary");
            return false;
        }
    };
    fill(page, CANARY, PAGE);
    let act_offset = 128;
    let old_offset = 512;
    // SAFETY: both offsets are 8-aligned struct slots inside the live page.
    let act = unsafe { &mut *((page + act_offset) as *mut Sigaction) };
    let old = unsafe { &mut *((page + old_offset) as *mut Sigaction) };
    *act = Sigaction {
        sa_handler: handler_address(),
        sa_mask: Sigset::ZERO,
        sa_flags: SA_SIGINFO,
        _pad: 0,
        sa_restorer: 0,
    };
    *old = Sigaction {
        sa_handler: 0,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: `act.sa_mask` is a live writable public sigset_t.
    let added = unsafe { sigaddset(&mut act.sa_mask, SIGUSR1) };

    let mut ok = added == 0;
    if ok {
        // SAFETY: both objects are live inside the canary page.
        ok = unsafe { sigaction(SIGUSR2, act, ptr::null_mut()) } == 0;
    }
    if ok {
        // SAFETY: `old` is a live writable public sigaction.
        ok = unsafe { sigaction(SIGUSR2, ptr::null(), old) } == 0;
    }
    if ok {
        // SAFETY: `old.sa_mask` is a live readable public sigset_t.
        let member = unsafe { sigismember(&old.sa_mask, SIGUSR1) } == 1;
        let intact = canaries_intact(
            page,
            &[
                (act_offset, act_offset + PUBLIC_SIGACTION),
                (old_offset, old_offset + PUBLIC_SIGACTION),
            ],
        );
        ok = old.sa_handler == handler_address()
            && old.sa_flags & SA_SIGINFO == SA_SIGINFO
            && member
            && intact;
        if ok {
            row_ok(b"canary", old.sa_flags, member);
        }
    }
    unmap(page, 1);
    if !ok {
        row_fail(b"canary");
    }
    ok
}

/// Block/read back/unblock `SIGUSR2` through the public `sigprocmask`.
fn procmask_row() -> bool {
    let mut set = Sigset::ZERO;
    let mut previous = Sigset::ZERO;
    // SAFETY: all three objects are live public sigset_t values.
    let mut ok = unsafe {
        sigemptyset(&mut set) == 0
            && sigaddset(&mut set, SIGUSR2) == 0
            && sigprocmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
            && sigprocmask(SIG_BLOCK, ptr::null(), &mut previous) == 0
    };
    if ok {
        // SAFETY: `previous` was filled by the successful sigprocmask readback.
        let member = unsafe { sigismember(&previous, SIGUSR2) } == 1;
        // SAFETY: `set` is a live public sigset_t and no old set is requested.
        ok = unsafe { sigprocmask(SIG_UNBLOCK, &set, ptr::null_mut()) == 0 } && member;
        if ok {
            procmask_ok(member);
        }
    }
    if !ok {
        row_fail(b"procmask");
    }
    ok
}

// S711 kernel provider rows: raw numbers from `crates/raven-abi/src/syscall.rs`, no libc
// disposition mirror in the path.
const RAW_SIGACTION: usize = 0x87;
const RAW_SIGPROCMASK: usize = 0x88;
const RAW_PROC_SIGNAL: usize = 0x4A;
const RAW_THREAD_SIGQUEUE: usize = 0x8A;
const RAW_SIGPENDING: usize = 0x8B;
const RAW_SIGWAIT: usize = 0x8C;
const HANDLE_NONE: usize = u32::MAX as usize;
const EINTR: c_int = 4;

/// The kernel's 40-byte `SIGACTION` request (`kernel/src/syscall/thread.rs`).
#[repr(C)]
#[derive(Clone, Copy)]
struct RawSigAction {
    handler: usize,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: usize,
}

const _: () = assert!(core::mem::size_of::<RawSigAction>() == 40);

const RAW_ACTION_ZERO: RawSigAction = RawSigAction {
    handler: 0,
    mask: [0; 2],
    flags: 0,
    _pad: 0,
    restorer: 0,
};

static HANDLER_COUNT: AtomicI32 = AtomicI32::new(0);
static HANDLER_SIG: AtomicI32 = AtomicI32::new(0);

extern "C" fn provider_handler(signal: c_int) {
    HANDLER_SIG.store(signal, Ordering::SeqCst);
    HANDLER_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Raw `syscall(number, a, b, c)` using the Raven register ABI.
fn raw3(number: usize, a: usize, b: usize, c: usize) -> isize {
    let result: isize;
    // SAFETY: the callers pass the documented register arguments for each raw number; the
    // asm block clobbers only the syscall scratch registers.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") a => result,
            in("x1") b,
            in("x2") c,
            options(nostack),
        );
    }
    result
}

fn raw_sigaction(signal: c_int, act: *const RawSigAction, old: *mut RawSigAction) -> isize {
    raw3(
        RAW_SIGACTION,
        signal as usize,
        act as usize,
        old as usize,
    )
}

fn raw_procmask(how: c_int, set: *const u64, old: *mut u64) -> isize {
    raw3(RAW_SIGPROCMASK, how as usize, set as usize, old as usize)
}

fn raw_signal(pid: c_int, signal: c_int) -> isize {
    raw3(RAW_PROC_SIGNAL, pid as usize, signal as usize, 0)
}

fn mask_bit(signal: c_int) -> u64 {
    1u64 << (signal as u32 - 1)
}

fn provider_ok(name: &[u8]) {
    let mut line = [0u8; 64];
    let mut at = 0;
    for byte in b"signal: provider " {
        line[at] = *byte;
        at += 1;
    }
    for byte in name {
        line[at] = *byte;
        at += 1;
    }
    for byte in b" ok\n" {
        line[at] = *byte;
        at += 1;
    }
    debug_write(&line[..at]);
}

fn provider_fail(name: &[u8]) {
    let mut line = [0u8; 64];
    let mut at = 0;
    for byte in b"signal: provider-fail " {
        line[at] = *byte;
        at += 1;
    }
    for byte in name {
        line[at] = *byte;
        at += 1;
    }
    line[at] = b'\n';
    at += 1;
    debug_write(&line[..at]);
}

fn install_provider_handler() -> bool {
    let action = RawSigAction {
        handler: provider_handler as *const () as usize,
        mask: [0; 2],
        flags: SA_SIGINFO as u32,
        _pad: 0,
        restorer: 0,
    };
    raw_sigaction(SIGUSR1, &action, ptr::null_mut()) == 0
}

fn provider_row(name: &[u8], ok: bool) -> bool {
    if ok {
        provider_ok(name);
    } else {
        provider_fail(name);
    }
    ok
}

/// Install through raw `SIGACTION` and read the stored action back.
fn provider_install_row() -> bool {
    let expected = provider_handler as *const () as usize;
    let mut previous = RAW_ACTION_ZERO;
    let ok = install_provider_handler()
        && raw_sigaction(SIGUSR1, ptr::null(), &mut previous) == 0
        && previous.handler == expected;
    provider_row(b"install", ok)
}

/// Generate through raw `PROC_SIGNAL` and run the kernel-built frame.
fn provider_generate_row() -> bool {
    let before = HANDLER_COUNT.load(Ordering::SeqCst);
    let mut ok = raw_signal(unsafe { getpid() }, SIGUSR1) == 0;
    ok = ok
        && HANDLER_COUNT.load(Ordering::SeqCst) == before + 1
        && HANDLER_SIG.load(Ordering::SeqCst) == SIGUSR1;
    provider_row(b"generate", ok)
}

/// A caught signal masked at generation is held, then delivered when the mask opens.
fn provider_blocked_row() -> bool {
    let set = mask_bit(SIGUSR1);
    let before = HANDLER_COUNT.load(Ordering::SeqCst);
    let mut ok = raw_procmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
        && raw_signal(unsafe { getpid() }, SIGUSR1) == 0
        && HANDLER_COUNT.load(Ordering::SeqCst) == before;
    ok = ok
        && raw_procmask(SIG_UNBLOCK, &set, ptr::null_mut()) == 0
        && HANDLER_COUNT.load(Ordering::SeqCst) == before + 1;
    provider_row(b"blocked", ok)
}

/// `SIG_IGN` discards, even though SIGUSR1's default action is fatal.
fn provider_ignored_row() -> bool {
    let ignored = RawSigAction {
        handler: 1,
        ..RAW_ACTION_ZERO
    };
    let before = HANDLER_COUNT.load(Ordering::SeqCst);
    let ok = raw_sigaction(SIGUSR1, &ignored, ptr::null_mut()) == 0
        && raw_signal(unsafe { getpid() }, SIGUSR1) == 0
        && HANDLER_COUNT.load(Ordering::SeqCst) == before
        && install_provider_handler();
    provider_row(b"ignored", ok)
}

/// `THREAD_SIGQUEUE` lands on the thread queue and `SIGPENDING` reports it.
fn provider_thread_row() -> bool {
    let set = mask_bit(SIGUSR2);
    let before = HANDLER_COUNT.load(Ordering::SeqCst);
    let default_action = RawSigAction {
        handler: (provider_handler as *const () as usize),
        ..RAW_ACTION_ZERO
    };
    let mut pending = 0u64;
    let mut ok = raw_sigaction(SIGUSR2, &default_action, ptr::null_mut()) == 0
        && raw_procmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
        && raw3(RAW_THREAD_SIGQUEUE, HANDLE_NONE, SIGUSR2 as usize, 0) == 0
        && raw3(RAW_SIGPENDING, &mut pending as *mut u64 as usize, 0, 0) == 1
        && pending & set != 0;
    ok = ok
        && raw_procmask(SIG_UNBLOCK, &set, ptr::null_mut()) == 0
        && HANDLER_COUNT.load(Ordering::SeqCst) == before + 1
        && HANDLER_SIG.load(Ordering::SeqCst) == SIGUSR2;
    provider_row(b"thread", ok)
}

/// `SIGWAIT` consumes a blocked pending record without running its handler.
fn provider_wait_row() -> bool {
    let set = mask_bit(SIGUSR1);
    let before = HANDLER_COUNT.load(Ordering::SeqCst);
    let mut out = [0u64; 6];
    let mut ok = raw_procmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
        && raw_signal(unsafe { getpid() }, SIGUSR1) == 0
        && raw3(
            RAW_SIGWAIT,
            &set as *const u64 as usize,
            out.as_mut_ptr() as usize,
            0,
        ) == SIGUSR1 as isize
        && out[0] == SIGUSR1 as u64
        && HANDLER_COUNT.load(Ordering::SeqCst) == before;
    ok = ok && raw_procmask(SIG_UNBLOCK, &set, ptr::null_mut()) == 0;
    provider_row(b"wait", ok)
}

/// A blocking read returns `EINTR` when a caught kernel signal arrives.
fn provider_interrupt_row() -> bool {
    let mut fds = [-1i32; 2];
    let mut ack = [-1i32; 2];
    let mut byte = 0u8;
    let mut status = 0i32;
    let mut result = 0isize;
    let mut error = 0i32;
    let before = HANDLER_COUNT.load(Ordering::SeqCst);
    // SAFETY: the fd pairs and byte are live stack values for the whole call.
    let mut ok = unsafe { pipe(fds.as_mut_ptr()) } == 0
        && unsafe { pipe(ack.as_mut_ptr()) } == 0;
    if !ok {
        return provider_row(b"interrupt", false);
    }
    // SAFETY: fork duplicates this process; the child signals and then waits on the ack pipe
    // so the data write end cannot close before the parent's read returns.
    let child = unsafe { fork() };
    if child == 0 {
        // SAFETY: the child owns fds[1] and ack[0] and exits without unwinding.
        unsafe {
            close(fds[0]);
            close(ack[1]);
            // The parent writes `go` immediately before entering read; after this byte plus a
            // bounded delay the reader is enrolled, so the signal interrupts the wait instead
            // of being delivered before it and leaving a silent block.
            let mut ack_byte = 0u8;
            let _ = read(ack[0], &mut ack_byte, 1);
            let delay = Timespec {
                tv_sec: 0,
                tv_nsec: 20_000_000,
            };
            let _ = nanosleep(&delay, ptr::null_mut());
            raw_signal(getppid(), SIGUSR1);
            // Keep the data write end open until the parent acknowledges the interrupted read.
            let _ = read(ack[0], &mut ack_byte, 1);
            close(ack[0]);
            _exit(0);
        }
    }
    ok = child > 0
        && unsafe { close(fds[1]) } == 0
        && unsafe { close(ack[0]) } == 0;
    if ok {
        let go = b'g';
        let done = b'd';
        let _ = unsafe { write(ack[1], &go as *const u8, 1) };
        // SAFETY: `byte` is a live one-byte buffer; the child keeps the data write end open
        // until the acknowledgement, so an empty pipe blocks here rather than returning EOF.
        result = unsafe { read(fds[0], &mut byte, 1) };
        // SAFETY: `__errno_location` returns the live thread-local errno pointer.
        error = unsafe { *__errno_location() };
        let _ = unsafe { write(ack[1], &done as *const u8, 1) };
        let _ = unsafe { close(fds[0]) };
        let _ = unsafe { close(ack[1]) };
        let _ = unsafe { waitpid(child, &mut status, 0) };
        ok = result == -1 && error == EINTR;
    }
    ok = ok
        && HANDLER_COUNT.load(Ordering::SeqCst) == before + 1
        && HANDLER_SIG.load(Ordering::SeqCst) == SIGUSR1;
    if !ok {
        // Diagnostic: result, errno and handler delta as hexadecimal words.
        let mut line = [0u8; 96];
        let mut at = 0;
        for byte in b"signal: provider-interrupt result=" {
            line[at] = *byte;
            at += 1;
        }
        let mut digits = [0u8; 8];
        let count = write_hex(result as u32, &mut digits);
        for index in (0..count).rev() {
            line[at] = digits[index];
            at += 1;
        }
        for byte in b" errno=" {
            line[at] = *byte;
            at += 1;
        }
        let count = write_hex(error as u32, &mut digits);
        for index in (0..count).rev() {
            line[at] = digits[index];
            at += 1;
        }
        for byte in b" handler=" {
            line[at] = *byte;
            at += 1;
        }
        let count = write_hex(HANDLER_COUNT.load(Ordering::SeqCst) as u32, &mut digits);
        for index in (0..count).rev() {
            line[at] = digits[index];
            at += 1;
        }
        line[at] = b'\n';
        at += 1;
        debug_write(&line[..at]);
    }
    provider_row(b"interrupt", ok)
}

fn run_provider() -> bool {
    provider_install_row()
        && provider_generate_row()
        && provider_blocked_row()
        && provider_ignored_row()
        && provider_thread_row()
        && provider_wait_row()
        && provider_interrupt_row()
}

fn run_boundary() -> bool {
    guard_row() && canary_row() && procmask_row()
}

/// `crt1.o` calls this after `__raven_libc_start`; the return value is the service exit status.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const u8) -> c_int {
    let boundary = run_boundary();
    if boundary {
        debug_write(b"signal: boundary ok\n");
    } else {
        debug_write(b"signal: boundary-fail rows\n");
    }
    let provider = run_provider();
    if boundary && provider {
        debug_write(b"signal: provider ok\n");
        0
    } else {
        1
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    debug_write(b"signal: boundary-fail panic\n");
    loop {
        core::hint::spin_loop();
    }
}
