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
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

const PAGE: usize = 4096;
const PUBLIC_SIGACTION: usize = 152;
const CANARY: u8 = 0x5a;

// `bits/signum.h` and `bits/abi_mman.h` from the installed sysroot.
const SIGUSR1: c_int = 10;
const SIGUSR2: c_int = 12;
const SA_ONSTACK: c_int = 0x1;
const SA_SIGINFO: c_int = 0x40;
const SIG_BLOCK: c_int = 1;
const SIG_UNBLOCK: c_int = 2;
const PROT_NONE: c_int = 0x0;
const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const MAP_PRIVATE: c_int = 0x2;
const MAP_ANON: c_int = 0x1000;
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
    fn sigaltstack(ss: *const Stack, old: *mut Stack) -> c_int;
    fn __errno_location() -> *mut c_int;
    fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> c_int;
    fn raise(sig: c_int) -> c_int;
    fn signal(sig: c_int, handler: usize) -> usize;
    fn kill(pid: c_int, sig: c_int) -> c_int;
    fn sigpending(set: *mut Sigset) -> c_int;
    fn sigsuspend(set: *const Sigset) -> c_int;
    fn sigwait(set: *const Sigset, sig: *mut c_int) -> c_int;
    fn pthread_create(
        thread: *mut usize,
        attr: *const c_void,
        start: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> c_int;
    fn pthread_join(thread: usize, value: *mut *mut c_void) -> c_int;
    fn pthread_self() -> usize;
    fn pthread_kill(thread: usize, sig: c_int) -> c_int;
    fn setpgid(pid: c_int, pgid: c_int) -> c_int;
}

const WNOHANG: c_int = 1;
const SIGKILL: c_int = 9;
const SIG_IGN: usize = 1;
const ONE_MS: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 1_000_000,
};

/// System V amd64 `stack_t` (24 bytes), the public C layout libc marshals.
#[repr(C)]
struct Stack {
    ss_sp: usize,
    ss_flags: c_int,
    _pad: c_int,
    ss_size: usize,
}

/// `SS_DISABLE`: the alternate signal stack is off.
const SS_DISABLE: c_int = 2;

#[repr(C, align(16))]
struct AlternateStack([u8; 8192]);

static mut EXEC_ALT: AlternateStack = AlternateStack([0; 8192]);

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
const ENOEXEC: isize = 8;

/// Raw `PROC_EXEC(request) -> 0` number (`userland/rt/src/exec.rs`).
const PROC_EXEC: usize = 0x6a;

/// S712 exec probe image, embedded at build time so the exec path needs no file scheme.
const EXEC_PROBE: &[u8] = include_bytes!(env!("RAVEN_EXEC_PROBE"));

/// One `PROC_EXEC` request (88 bytes; `userland/rt/src/exec.rs:ExecRequest`).
#[repr(C)]
struct ExecRequest {
    image: u64,
    image_len: u64,
    path: u64,
    path_len: u64,
    argv: u64,
    argc: u64,
    envp: u64,
    envc: u64,
    close_list: u64,
    close_count: u64,
    flags: u64,
}

const _: () = assert!(core::mem::size_of::<ExecRequest>() == 88);

/// Replace this image through raw `PROC_EXEC`.
///
/// `arg0` must be NUL-terminated; the path length is explicit, matching the kernel request.
fn exec_image(image: &[u8], path: &[u8], arg0: &[u8]) -> isize {
    let argv: [*const u8; 1] = [arg0.as_ptr()];
    let request = ExecRequest {
        image: image.as_ptr() as u64,
        image_len: image.len() as u64,
        path: path.as_ptr() as u64,
        path_len: path.len() as u64,
        argv: argv.as_ptr() as u64,
        argc: 1,
        envp: 0,
        envc: 0,
        close_list: 0,
        close_count: 0,
        flags: 0,
    };
    // SAFETY: the request, image, path and NUL-terminated argv are live for the call; the kernel
    // validates every user range and copies only the declared lengths.
    unsafe {
        raw3(
            PROC_EXEC,
            &request as *const ExecRequest as usize,
            0,
            0,
        )
    }
}

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
                tv_nsec: 100_000_000,
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

/// Signal state the exec transition must keep across a **failed** exec.
#[derive(Clone, Copy, PartialEq, Eq)]
struct SignalState {
    caught: bool,
    ignored: bool,
    blocked: bool,
    altstack_on: bool,
}

/// Read the dispositions, thread mask and altstack through the public libc calls.
fn current_signal_state() -> Option<SignalState> {
    let mut us1 = Sigaction {
        sa_handler: 0,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    let mut us2 = Sigaction {
        sa_handler: 0,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    let mut mask = Sigset::ZERO;
    let mut stack = Stack {
        ss_sp: 0,
        ss_flags: 0,
        _pad: 0,
        ss_size: 0,
    };
    // SAFETY: every object is live and each call only reads (`act` and `ss` are NULL, `how` is
    // ignored without a set).
    let queried = unsafe {
        sigaction(SIGUSR1, ptr::null(), &mut us1) == 0
            && sigaction(SIGUSR2, ptr::null(), &mut us2) == 0
            && sigprocmask(SIG_BLOCK, ptr::null(), &mut mask) == 0
            && sigaltstack(ptr::null(), &mut stack) == 0
    };
    if !queried {
        return None;
    }
    // SAFETY: `mask` was filled by the successful sigprocmask readback.
    Some(SignalState {
        caught: us1.sa_handler == handler_address(),
        ignored: us2.sa_handler == 1,
        blocked: unsafe { sigismember(&mask, SIGUSR2) } == 1,
        altstack_on: stack.ss_flags & SS_DISABLE == 0,
    })
}

/// Emit `signal: exec-errno=0x<hex>` after a failed successful-path exec.
fn debug_exec_errno(error: c_int) {
    let mut line = [0u8; 48];
    let mut at = 0;
    for byte in b"signal: exec-errno=0x" {
        line[at] = *byte;
        at += 1;
    }
    let mut digits = [0u8; 8];
    let count = write_hex(error as u32, &mut digits);
    for index in (0..count).rev() {
        line[at] = digits[index];
        at += 1;
    }
    line[at] = b'\n';
    at += 1;
    debug_write(&line[..at]);
}

/// S712 step 4: a failed exec leaves the old image's signal state coherent; a successful exec
/// resets caught handlers, keeps `SIG_IGN` and the mask, and disables the alternate stack.
///
/// The success half never returns: the service image is replaced by `/bin/exec-probe`, which
/// reads the kernel table of the new image and exits with the row result.
fn exec_transition() -> bool {
    let caught = Sigaction {
        sa_handler: handler_address(),
        sa_mask: Sigset::ZERO,
        sa_flags: SA_SIGINFO,
        _pad: 0,
        sa_restorer: 0,
    };
    let ignored = Sigaction {
        sa_handler: 1,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    let mut set = Sigset::ZERO;
    let stack = Stack {
        ss_sp: core::ptr::addr_of!(EXEC_ALT) as usize,
        ss_flags: 0,
        _pad: 0,
        ss_size: core::mem::size_of::<AlternateStack>(),
    };
    // SAFETY: every object is live; `sigaddset` fills `set`, `sigaltstack` reads `stack`, and
    // the disposition/mask calls use the public libc entry points.
    let installed = unsafe {
        sigemptyset(&mut set) == 0
            && sigaddset(&mut set, SIGUSR2) == 0
            && sigaction(SIGUSR1, &caught, ptr::null_mut()) == 0
            && sigaction(SIGUSR2, &ignored, ptr::null_mut()) == 0
            && sigprocmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
            && sigaltstack(&stack, ptr::null_mut()) == 0
    };
    if !installed {
        debug_write(b"signal: provider-fail exec-install\n");
        return false;
    }
    let before = current_signal_state();
    // A malformed image fails ELF validation before the address space is touched; the old
    // image's dispositions, mask and altstack must still be exactly what they were.
    let failed =
        exec_image(b"not-an-executable-image", b"/bin/exec-probe-missing", b"missing\0")
            == -ENOEXEC;
    let after = current_signal_state();
    let coherent = failed && before.is_some() && before == after;
    if !coherent {
        debug_write(b"signal: provider-fail exec-failure\n");
        return false;
    }
    debug_write(b"signal: exec-failure ok\n");
    let result = exec_image(EXEC_PROBE, b"/bin/exec-probe", b"exec-probe\0");
    debug_exec_errno((-result) as c_int);
    debug_write(b"signal: provider-fail exec-success\n");
    false
}

/// S720: a disposition installed through public `sigaction` must be the one `raise` delivers.
///
/// The pre-fix libc raised from its own `DISP` table, so the child's default action ran and the
/// child exited normally with 138 instead of running the handler. The parent observes the wait
/// status with the POSIX macros, never the shell's numeric convention.
static S720_HANDLER_COUNT: AtomicI32 = AtomicI32::new(0);

extern "C" fn s720_handler(_signal: c_int) {
    S720_HANDLER_COUNT.fetch_add(1, Ordering::SeqCst);
}

fn s720_raise_sigaction_row() -> bool {
    // SAFETY: fork duplicates the process; the child only touches its own copy of the statics.
    let child = unsafe { fork() };
    if child == 0 {
        let action = Sigaction {
            sa_handler: s720_handler as *const () as usize,
            sa_mask: Sigset::ZERO,
            sa_flags: 0,
            _pad: 0,
            sa_restorer: 0,
        };
        // SAFETY: both objects are live for the call in the child.
        let installed = unsafe { sigaction(SIGUSR1, &action, ptr::null_mut()) } == 0;
        // SAFETY: raise takes one scalar signal number.
        let raised = installed && unsafe { raise(SIGUSR1) } == 0;
        let ran = S720_HANDLER_COUNT.load(Ordering::SeqCst) == 1;
        // SAFETY: _exit never returns and does not unwind the child.
        unsafe { _exit(if raised && ran { 0 } else { 1 }) };
    }
    let mut status = 0;
    // SAFETY: status is a live writable c_int for this waitpid call.
    let waited = unsafe { waitpid(child, &mut status, 0) } == child;
    let exited = status & 0x7f == 0;
    let code = (status >> 8) & 0xff;
    debug_write(b"signal: s720 status=");
    debug_decimal(status as u32);
    debug_write(b" exited=");
    debug_decimal(u32::from(exited));
    debug_write(b" code=");
    debug_decimal(code as u32);
    debug_write(b" signaled=");
    debug_decimal(u32::from(!exited));
    debug_write(b"\n");
    provider_row(b"s720-raise-sigaction", waited && exited && code == 0)
}

/// Emit one decimal integer on the debug sink.
fn debug_decimal(mut value: u32) {
    let mut digits = [0u8; 10];
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    debug_write(&digits[at..]);
}

fn wait_status_exited(status: c_int) -> bool {
    status & 0x7f == 0
}

fn wait_status_code(status: c_int) -> c_int {
    (status >> 8) & 0xff
}

fn sleep_ms(milliseconds: u32) {
    let mut left = milliseconds;
    while left != 0 {
        // SAFETY: both Timespec values are live for the call.
        let _ = unsafe { nanosleep(&ONE_MS, ptr::null_mut()) };
        left -= 1;
    }
}

/// Wait for `child` for at most `milliseconds`; kill it and return `None` on expiry.
fn wait_bounded(child: c_int, milliseconds: u32) -> Option<c_int> {
    let mut status = 0;
    let mut left = milliseconds;
    while left != 0 {
        // SAFETY: `status` is a live writable c_int.
        let result = unsafe { waitpid(child, &mut status, WNOHANG) };
        if result == child {
            return Some(status);
        }
        if result < 0 {
            return None;
        }
        sleep_ms(1);
        left -= 1;
    }
    // SAFETY: the child pid is live; SIGKILL releases a row that would otherwise hang.
    unsafe {
        kill(child, SIGKILL);
        waitpid(child, &mut status, 0);
    }
    None
}

fn wait_counter(counter: &AtomicI32, target: i32, milliseconds: u32) -> bool {
    let mut left = milliseconds;
    while left != 0 {
        if counter.load(Ordering::SeqCst) >= target {
            return true;
        }
        sleep_ms(1);
        left -= 1;
    }
    counter.load(Ordering::SeqCst) >= target
}

fn s720_child_ok(child: c_int) -> bool {
    wait_bounded(child, 3000).is_some_and(|status| wait_status_exited(status) && wait_status_code(status) == 0)
}

/// `signal()` must install the kernel disposition, not only libc's table.
fn s720_signal_caught_row() -> bool {
    // SAFETY: fork duplicates the process; the child only mutates its own statics.
    let child = unsafe { fork() };
    if child == 0 {
        S720_HANDLER_COUNT.store(0, Ordering::SeqCst);
        // SAFETY: the handler address is a live extern "C" function.
        let installed = unsafe { signal(SIGUSR2, s720_handler as *const () as usize) } != usize::MAX;
        let sent = unsafe { raw_signal(getpid(), SIGUSR2) } == 0;
        let ran = S720_HANDLER_COUNT.load(Ordering::SeqCst) == 1;
        // SAFETY: _exit never returns.
        unsafe { _exit(if installed && sent && ran { 0 } else { 1 }) };
    }
    provider_row(b"s720-signal-caught", s720_child_ok(child))
}

/// `SIG_IGN` installed through `signal()` must survive kernel delivery.
fn s720_signal_ignored_row() -> bool {
    // SAFETY: as above.
    let child = unsafe { fork() };
    if child == 0 {
        // SAFETY: signal takes a scalar handler word.
        let installed = unsafe { signal(SIGUSR2, SIG_IGN) } != usize::MAX;
        let sent = unsafe { raw_signal(getpid(), SIGUSR2) } == 0;
        // SAFETY: _exit never returns.
        unsafe { _exit(if installed && sent { 0 } else { 1 }) };
    }
    provider_row(b"s720-signal-ignored", s720_child_ok(child))
}

/// A blocked process-directed signal must be visible through `sigpending`.
fn s720_pending_row() -> bool {
    // SAFETY: as above.
    let child = unsafe { fork() };
    if child == 0 {
        S720_HANDLER_COUNT.store(0, Ordering::SeqCst);
        let mut set = Sigset::ZERO;
        let mut pending = Sigset::ZERO;
        // SAFETY: the sigset objects are live for each call.
        let mut ok = unsafe {
            sigemptyset(&mut set) == 0
                && sigaddset(&mut set, SIGUSR1) == 0
                && sigaction(SIGUSR1, &signal_action(), ptr::null_mut()) == 0
                && sigprocmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
                && raw_signal(getpid(), SIGUSR1) == 0
                && sigpending(&mut pending) == 0
                && sigismember(&pending, SIGUSR1) == 1
                && sigprocmask(SIG_UNBLOCK, &set, ptr::null_mut()) == 0
        };
        ok = ok && S720_HANDLER_COUNT.load(Ordering::SeqCst) == 1;
        // SAFETY: _exit never returns.
        unsafe { _exit(if ok { 0 } else { 1 }) };
    }
    provider_row(b"s720-pending", s720_child_ok(child))
}

/// `sigsuspend` must not lose an already-pending signal at the temporary mask change.
fn s720_sigsuspend_row() -> bool {
    // SAFETY: as above.
    let child = unsafe { fork() };
    if child == 0 {
        S720_HANDLER_COUNT.store(0, Ordering::SeqCst);
        let mut set = Sigset::ZERO;
        // SAFETY: the sigset object is live for the calls.
        let prepared = unsafe {
            sigemptyset(&mut set) == 0
                && sigaddset(&mut set, SIGUSR1) == 0
                && sigaction(SIGUSR1, &signal_action(), ptr::null_mut()) == 0
                && sigprocmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
                && raw_signal(getpid(), SIGUSR1) == 0
        };
        // The empty mask makes SIGUSR1 deliverable while the call waits.
        let result = if prepared {
            // SAFETY: the empty sigset is live for the call.
            unsafe { sigsuspend(&Sigset::ZERO) }
        } else {
            -1
        };
        // SAFETY: the errno cell belongs to this thread.
        let error = unsafe { *__errno_location() };
        let ok = prepared
            && result == -1
            && error == EINTR
            && S720_HANDLER_COUNT.load(Ordering::SeqCst) == 1;
        // SAFETY: _exit never returns.
        unsafe { _exit(if ok { 0 } else { 1 }) };
    }
    provider_row(b"s720-sigsuspend", s720_child_ok(child))
}

/// A blocked temporary mask must wake from the kernel wait when another process sends.
fn s720_sigsuspend_wait_row() -> bool {
    S720_HANDLER_COUNT.store(0, Ordering::SeqCst);
    let mut set = Sigset::ZERO;
    // SAFETY: the sigset object is live for the calls.
    let prepared = unsafe {
        sigemptyset(&mut set) == 0
            && sigaddset(&mut set, SIGUSR1) == 0
            && sigaction(SIGUSR1, &signal_action(), ptr::null_mut()) == 0
            && sigprocmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
    };
    if !prepared {
        return provider_row(b"s720-sigsuspend-wait", false);
    }
    // SAFETY: fork duplicates the process; the child only sleeps once and signals the parent.
    let child = unsafe { fork() };
    if child == 0 {
        sleep_ms(20);
        // SAFETY: the parent pid is live and the call takes two scalars.
        let _ = unsafe { raw_signal(getppid(), SIGUSR1) };
        // SAFETY: _exit never returns.
        unsafe { _exit(0) };
    }
    // SAFETY: the empty sigset is live for the call.
    let result = unsafe { sigsuspend(&Sigset::ZERO) };
    // SAFETY: the errno cell belongs to this thread.
    let error = unsafe { *__errno_location() };
    // Restore the pre-row empty mask; this row runs in the fixture process, not a child.
    // SAFETY: `set` is live and only names SIGUSR1.
    unsafe {
        let _ = sigprocmask(SIG_UNBLOCK, &set, ptr::null_mut());
    }
    let child_ok = wait_bounded(child, 3000)
        .is_some_and(|status| wait_status_exited(status) && wait_status_code(status) == 0);
    let ok = result == -1
        && error == EINTR
        && S720_HANDLER_COUNT.load(Ordering::SeqCst) == 1
        && child_ok;
    provider_row(b"s720-sigsuspend-wait", ok)
}

/// `sigwait` must consume the kernel record without running the handler.
fn s720_sigwait_row() -> bool {
    // SAFETY: as above.
    let child = unsafe { fork() };
    if child == 0 {
        S720_HANDLER_COUNT.store(0, Ordering::SeqCst);
        let mut set = Sigset::ZERO;
        let mut received: c_int = 0;
        // SAFETY: the sigset and out-parameter are live for the calls.
        let prepared = unsafe {
            sigemptyset(&mut set) == 0
                && sigaddset(&mut set, SIGUSR1) == 0
                && sigaction(SIGUSR1, &signal_action(), ptr::null_mut()) == 0
                && sigprocmask(SIG_BLOCK, &set, ptr::null_mut()) == 0
                && raw_signal(getpid(), SIGUSR1) == 0
        };
        let result = if prepared {
            // SAFETY: the set and output number are live for the call.
            unsafe { sigwait(&set, &mut received) }
        } else {
            -1
        };
        let ok = prepared
            && result == 0
            && received == SIGUSR1
            && S720_HANDLER_COUNT.load(Ordering::SeqCst) == 0;
        // SAFETY: _exit never returns.
        unsafe { _exit(if ok { 0 } else { 1 }) };
    }
    provider_row(b"s720-sigwait", s720_child_ok(child))
}

fn signal_action() -> Sigaction {
    Sigaction {
        sa_handler: s720_handler as *const () as usize,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    }
}

/// `ucontext_t.uc_mcontext.gregs[REG_RIP]` on x86-64 (System V public layout).
#[cfg(target_arch = "x86_64")]
const S720_UC_PC_OFFSET: usize = 40 + 16 * 8;
/// `ucontext_t.uc_mcontext.__gregs[REG_ELR]` in the C19 aarch64 kernel frame.
#[cfg(target_arch = "aarch64")]
const S720_UC_PC_OFFSET: usize = 0x10 + 32 * 8;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const S720_UC_PC_OFFSET: usize = 0;
static S720_UC_PC: AtomicUsize = AtomicUsize::new(0);
static S720_UC_RAN: AtomicI32 = AtomicI32::new(0);

extern "C" fn s720_ucontext_handler(_signal: c_int, _info: *mut c_void, context: *mut c_void) {
    S720_UC_RAN.store(1, Ordering::SeqCst);
    if context.is_null() {
        return;
    }
    // SAFETY: the kernel passes a live `ucontext_t`; the offset names the architecture's
    // interrupted-PC word in the C19 frame (x86-64 RIP, aarch64 ELR).
    let pc = unsafe {
        core::ptr::read_volatile((context as *const u8).add(S720_UC_PC_OFFSET) as *const u64)
    };
    S720_UC_PC.store(pc as usize, Ordering::SeqCst);
}

static A_READY: AtomicI32 = AtomicI32::new(0);
static B_READY: AtomicI32 = AtomicI32::new(0);
static A_RELEASE: AtomicI32 = AtomicI32::new(0);
static B_RELEASE: AtomicI32 = AtomicI32::new(0);
static TARGET_A_COUNT: AtomicI32 = AtomicI32::new(0);
static TARGET_B_COUNT: AtomicI32 = AtomicI32::new(0);
static TARGET_A_TID: AtomicUsize = AtomicUsize::new(0);
static TARGET_B_TID: AtomicUsize = AtomicUsize::new(0);

extern "C" fn target_handler(signal_number: c_int) {
    if signal_number == SIGUSR2 {
        TARGET_A_TID.store(unsafe { pthread_self() }, Ordering::SeqCst);
        TARGET_A_COUNT.fetch_add(1, Ordering::SeqCst);
    } else {
        TARGET_B_TID.store(unsafe { pthread_self() }, Ordering::SeqCst);
        TARGET_B_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

extern "C" fn target_thread(arg: *mut c_void) -> *mut c_void {
    let which = arg as usize;
    let (blocked, unblocked) = if which == 0 {
        (SIGUSR2, SIGUSR1)
    } else {
        (SIGUSR1, SIGUSR2)
    };
    let mut block = Sigset::ZERO;
    let mut open = Sigset::ZERO;
    // SAFETY: the sigset objects are live for the calls; each thread owns its own mask.
    unsafe {
        let _ = sigemptyset(&mut block);
        let _ = sigaddset(&mut block, blocked);
        let _ = sigemptyset(&mut open);
        let _ = sigaddset(&mut open, unblocked);
        let _ = sigprocmask(SIG_BLOCK, &block, ptr::null_mut());
        let _ = sigprocmask(SIG_UNBLOCK, &open, ptr::null_mut());
    }
    if which == 0 {
        A_READY.store(1, Ordering::SeqCst);
        while A_RELEASE.load(Ordering::SeqCst) == 0 {
            core::hint::spin_loop();
        }
    } else {
        B_READY.store(1, Ordering::SeqCst);
        while B_RELEASE.load(Ordering::SeqCst) == 0 {
            core::hint::spin_loop();
        }
    }
    // SAFETY: the thread's own sigset is live; the pending signal is delivered at this return.
    unsafe {
        let _ = sigprocmask(SIG_UNBLOCK, &block, ptr::null_mut());
    }
    ptr::null_mut()
}

fn wait_flag(flag: &AtomicI32, milliseconds: u32) -> bool {
    wait_counter(flag, 1, milliseconds)
}

/// Two threads with opposite masks: delivery must stay on the thread named by `pthread_kill`.
fn s720_pthread_kill_row() -> bool {
    let action = Sigaction {
        sa_handler: target_handler as *const () as usize,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: the action is live for each install.
    let installed = unsafe {
        sigaction(SIGUSR1, &action, ptr::null_mut()) == 0
            && sigaction(SIGUSR2, &action, ptr::null_mut()) == 0
    };
    if !installed {
        return provider_row(b"s720-pthread-kill", false);
    }
    TARGET_A_COUNT.store(0, Ordering::SeqCst);
    TARGET_B_COUNT.store(0, Ordering::SeqCst);
    TARGET_A_TID.store(0, Ordering::SeqCst);
    TARGET_B_TID.store(0, Ordering::SeqCst);
    A_READY.store(0, Ordering::SeqCst);
    B_READY.store(0, Ordering::SeqCst);
    A_RELEASE.store(0, Ordering::SeqCst);
    B_RELEASE.store(0, Ordering::SeqCst);
    let mut a_thread = 0usize;
    let mut b_thread = 0usize;
    // SAFETY: both thread handles and the null attribute are live/valid for the calls.
    let created_a = unsafe {
        pthread_create(
            &mut a_thread,
            ptr::null(),
            target_thread,
            0usize as *mut c_void,
        ) == 0
    };
    let created_b = unsafe {
        pthread_create(
            &mut b_thread,
            ptr::null(),
            target_thread,
            1usize as *mut c_void,
        ) == 0
    };
    let ready = created_a && created_b && wait_flag(&A_READY, 1000) && wait_flag(&B_READY, 1000);
    // A blocks SIGUSR2; B and main do not. A process-directed send would run on B or main now.
    let kill_a_result = if ready {
        unsafe { pthread_kill(a_thread, SIGUSR2) }
    } else {
        -1
    };
    let kill_a = kill_a_result == 0;
    sleep_ms(50);
    let quiet_a = TARGET_A_COUNT.load(Ordering::SeqCst) == 0;
    let quiet_b = TARGET_B_COUNT.load(Ordering::SeqCst) == 0;
    A_RELEASE.store(1, Ordering::SeqCst);
    let ran_a = wait_counter(&TARGET_A_COUNT, 1, 1000);
    let mut result: *mut c_void = ptr::null_mut();
    // SAFETY: the thread handle is live until the join.
    let joined_a = unsafe { pthread_join(a_thread, &mut result) } == 0;
    let count_a = TARGET_A_COUNT.load(Ordering::SeqCst);
    let tid_a = TARGET_A_TID.load(Ordering::SeqCst);
    // B blocks SIGUSR1; A has exited and main does not block it.
    let kill_b_result = unsafe { pthread_kill(b_thread, SIGUSR1) };
    let kill_b = kill_b_result == 0;
    sleep_ms(50);
    let quiet_b2 = TARGET_B_COUNT.load(Ordering::SeqCst) == 0;
    B_RELEASE.store(1, Ordering::SeqCst);
    let ran_b = wait_counter(&TARGET_B_COUNT, 1, 1000);
    // SAFETY: the thread handle is live until the join.
    let joined_b = unsafe { pthread_join(b_thread, &mut result) } == 0;
    let count_b = TARGET_B_COUNT.load(Ordering::SeqCst);
    let tid_b = TARGET_B_TID.load(Ordering::SeqCst);
    if !created_a || !created_b || !ready || !kill_a || !quiet_a || !quiet_b || !ran_a || !joined_a
        || count_a != 1 || tid_a != a_thread || !kill_b || !quiet_b2 || !ran_b || !joined_b
        || count_b != 1 || tid_b != b_thread
    {
        debug_write(b"signal: s720-pk ca=");
        debug_decimal(u32::from(created_a));
        debug_write(b" cb=");
        debug_decimal(u32::from(created_b));
        debug_write(b" ready=");
        debug_decimal(u32::from(ready));
        debug_write(b" ka=");
        debug_decimal(kill_a_result as u32);
        debug_write(b" qa=");
        debug_decimal(u32::from(quiet_a));
        debug_write(b" qb=");
        debug_decimal(u32::from(quiet_b));
        debug_write(b" ra=");
        debug_decimal(u32::from(ran_a));
        debug_write(b" ja=");
        debug_decimal(u32::from(joined_a));
        debug_write(b" ca=");
        debug_decimal(count_a as u32);
        debug_write(b" ta=");
        debug_decimal(tid_a as u32);
        debug_write(b" kb=");
        debug_decimal(kill_b_result as u32);
        debug_write(b" qb2=");
        debug_decimal(u32::from(quiet_b2));
        debug_write(b" rb=");
        debug_decimal(u32::from(ran_b));
        debug_write(b" jb=");
        debug_decimal(u32::from(joined_b));
        debug_write(b" cb=");
        debug_decimal(count_b as u32);
        debug_write(b" tb=");
        debug_decimal(tid_b as u32);
        debug_write(b"\n");
    }
    provider_row(
        b"s720-pthread-kill",
        created_a
            && created_b
            && ready
            && kill_a
            && quiet_a
            && quiet_b
            && ran_a
            && joined_a
            && count_a == 1
            && tid_a == a_thread
            && kill_b
            && quiet_b2
            && ran_b
            && joined_b
            && count_b == 1
            && tid_b == b_thread,
    )
}

/// A disposition installed only in the kernel (raw `SIGACTION`) must still be the one `raise`
/// delivers; this is the exact split-store case the libc-local `DISP` table missed.
fn s720_raise_kernel_action_row() -> bool {
    // SAFETY: fork duplicates the process; the child only mutates its own statics.
    let child = unsafe { fork() };
    if child == 0 {
        S720_HANDLER_COUNT.store(0, Ordering::SeqCst);
        let action = RawSigAction {
            handler: s720_handler as *const () as usize,
            ..RAW_ACTION_ZERO
        };
        let installed = raw_sigaction(SIGUSR1, &action, ptr::null_mut()) == 0;
        // SAFETY: raise takes one scalar signal number.
        let raised = installed && unsafe { raise(SIGUSR1) } == 0;
        let ran = S720_HANDLER_COUNT.load(Ordering::SeqCst) == 1;
        // SAFETY: _exit never returns.
        unsafe { _exit(if raised && ran { 0 } else { 1 }) };
    }
    provider_row(b"s720-raise-kernel-action", s720_child_ok(child))
}

/// The kernel-delivered `ucontext_t` must point at the interrupted instruction in the text image.
///
/// This is the S720 replacement for the C fixture's old proximity check to `&raise`: with
/// thread-directed kernel delivery the saved PC is the libc instruction after the syscall.
fn s720_ucontext_row() -> bool {
    // SAFETY: fork duplicates the process; the child only mutates its own statics.
    let child = unsafe { fork() };
    if child == 0 {
        S720_UC_PC.store(0, Ordering::SeqCst);
        S720_UC_RAN.store(0, Ordering::SeqCst);
        let action = Sigaction {
            sa_handler: s720_ucontext_handler as *const () as usize,
            sa_mask: Sigset::ZERO,
            sa_flags: SA_SIGINFO,
            _pad: 0,
            sa_restorer: 0,
        };
        // SAFETY: the action is live for the call.
        let installed = unsafe { sigaction(SIGUSR1, &action, ptr::null_mut()) } == 0;
        // SAFETY: raise takes one scalar signal number.
        let raised = installed && unsafe { raise(SIGUSR1) } == 0;
        let ran = S720_UC_RAN.load(Ordering::SeqCst) == 1;
        let pc = S720_UC_PC.load(Ordering::SeqCst);
        let text_high = b"signal: provider ok".as_ptr() as usize;
        let ok = raised && ran && pc >= 0x400000 && pc < text_high;
        // SAFETY: _exit never returns.
        unsafe { _exit(if ok { 0 } else { 1 }) };
    }
    provider_row(b"s720-ucontext", s720_child_ok(child))
}

fn run_provider() -> bool {
    s720_raise_sigaction_row()
        && s720_raise_kernel_action_row()
        && s720_ucontext_row()
        && s720_signal_caught_row()
        && s720_signal_ignored_row()
        && s720_pending_row()
        && s720_sigsuspend_row()
        && s720_sigsuspend_wait_row()
        && s720_sigwait_row()
        && s720_pthread_kill_row()
        && provider_install_row()
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
    // Isolate the fixture in its own process group: the S720 matrix uses process-directed
    // delivery as a control, and a group-wide send must never reach svcd or the other services.
    // SAFETY: setpgid takes two scalar ids; failure leaves the inherited group unchanged.
    unsafe {
        setpgid(0, 0);
    }
    let boundary = run_boundary();
    if boundary {
        debug_write(b"signal: boundary ok\n");
    } else {
        debug_write(b"signal: boundary-fail rows\n");
    }
    let provider = run_provider();
    if boundary && provider {
        debug_write(b"signal: provider ok\n");
        // S712 step 4: the old image now carries a caught handler, an ignored disposition, a
        // blocked signal and an alternate stack. A failed exec must keep them; a successful one
        // must reset/keep/disable them in the probe image.
        if exec_transition() {
            return 1;
        }
        1
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
