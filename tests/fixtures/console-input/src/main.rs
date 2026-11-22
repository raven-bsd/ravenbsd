// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K516 `SYS_DEBUG_READ` contract fixture.
//!
//! The rows run in serial-console order: zero length, a bad user pointer with queued bytes,
//! a small buffer over a ten-byte line, a 64-byte burst read in eight-byte chunks, a timed
//! empty read, and a cross-process `PROC_SIGNAL` that interrupts a blocked read. Every marker
//! is printed only after the bytes were checked in the guest.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscalls; every unsafe block carries a SAFETY comment.

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const PROC_FORK: usize = 0x39;
const PROC_HANDLE_INFO: usize = 0x3E;
const PROC_WAIT_EX: usize = 0x48;
const PROC_SIGNAL: usize = 0x4A;
const PROC_SIGRECV: usize = 0x4B;
const P_PID: usize = 1;
const SIGUSR1: usize = 10;

/// Let the qtest dialogue deliver the bytes before the guest reads them.
const SETTLE_NS: u64 = 500_000_000;
/// Generous bound for one row; the contract only says a read must not wait unbounded.
const READ_TIMEOUT_NS: u64 = 1_000_000_000;
/// Timeout used by the timed-empty row.
const EMPTY_TIMEOUT_NS: u64 = 100_000_000;
const EMPTY_MIN_NS: u64 = 50_000_000;
const EMPTY_MAX_NS: u64 = 3_000_000_000;

/// Exactly 64 bytes; the qtest dialogue sends this string verbatim.
const BURST: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz+-";

#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn syscall6(
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
fn syscall6(
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

fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn debug_read_at(address: usize, length: usize, timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: DEBUG_READ validates the user range and (after the K516 fix) the writable VMA
    // before it touches the address; the caller owns the buffer behind `address`.
    checked(syscall6(
        Syscall::DEBUG_READ as usize,
        address,
        length,
        timeout_ns as usize,
        0,
        0,
        0,
    ))
}

fn read_into(buffer: &mut [u8], timeout_ns: u64) -> Result<usize, Errno> {
    debug_read_at(buffer.as_mut_ptr() as usize, buffer.len(), timeout_ns)
}

/// Fill `buffer`, stopping on a zero count (timeout) or an error.
fn read_exact(buffer: &mut [u8], timeout_ns: u64) -> Result<usize, Errno> {
    let mut used = 0usize;
    while used < buffer.len() {
        let Some(rest) = buffer.get_mut(used..) else {
            return Err(Errno::EINVAL);
        };
        let count = read_into(rest, timeout_ns)?;
        if count == 0 {
            break;
        }
        if count > rest.len() {
            return Err(Errno::EFAULT);
        }
        used = used.saturating_add(count);
    }
    Ok(used)
}

fn clock_ns() -> Result<u64, Errno> {
    let timespec = sys::clock_get(clock::MONOTONIC)?;
    let seconds = u64::try_from(timespec.tv_sec).map_err(|_| Errno::EINVAL)?;
    let nanos = u64::try_from(timespec.tv_nsec).map_err(|_| Errno::EINVAL)?;
    Ok(seconds.saturating_mul(1_000_000_000).saturating_add(nanos))
}

fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    checked(syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0)).map(|value| Handle(value as u32))
}

fn handle_info(handle: Handle) -> Result<usize, Errno> {
    checked(syscall6(PROC_HANDLE_INFO, handle.0 as usize, 0, 0, 0, 0, 0))
}

fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    checked(syscall6(
        Syscall::THREAD_CREATE as usize,
        process.0 as usize,
        entry,
        stack_top,
        arg,
        0,
        0,
    ))
    .map(|value| Handle(value as u32))
}

fn signal(pid: i32, signo: usize) -> Result<(), Errno> {
    checked(syscall6(
        PROC_SIGNAL,
        pid as isize as usize,
        signo,
        0,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

fn sigrecv() -> Result<usize, Errno> {
    let mut info = [0u64; 3];
    checked(syscall6(
        PROC_SIGRECV,
        info.as_mut_ptr() as usize,
        0,
        0,
        0,
        0,
        0,
    ))
}

fn wait_ex(idtype: usize, id: i32, options: usize, status: &mut i32) -> Result<usize, Errno> {
    checked(syscall6(
        PROC_WAIT_EX,
        idtype,
        id as isize as usize,
        options,
        status as *mut i32 as usize,
        0,
        0,
    ))
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

fn zero_row() -> Result<(), &'static str> {
    let count = debug_read_at(0, 0, 0).map_err(|_| "zero")?;
    if count != 0 {
        return Err("zero-count");
    }
    println!("console-input: zero=0");
    Ok(())
}

fn bad_pointer_row() -> Result<(), &'static str> {
    let page = sys::map_anonymous(PAGE).map_err(|_| "bad-map")?;
    sys::unmap(Handle::SELF_PROC, page, PAGE).map_err(|_| "bad-unmap")?;
    println!("console-input: ready-bad");
    sys::nanosleep(SETTLE_NS).map_err(|_| "bad-sleep")?;
    match debug_read_at(page, 6, READ_TIMEOUT_NS) {
        Err(Errno::EFAULT) => {}
        Err(_) => return Err("bad-errno"),
        Ok(_) => return Err("bad-success"),
    }
    println!("console-input: bad=EFAULT");
    let mut tail = [0u8; 6];
    let count = read_exact(&mut tail, READ_TIMEOUT_NS).map_err(|_| "bad-read")?;
    if count != tail.len() || &tail != b"BADPTR" {
        return Err("bad-tail");
    }
    println!("console-input: bad-tail=1");
    Ok(())
}

fn bounds_row() -> Result<(), &'static str> {
    println!("console-input: ready-bounds");
    sys::nanosleep(SETTLE_NS).map_err(|_| "bounds-sleep")?;
    let mut first = [0u8; 4];
    let mut second = [0u8; 4];
    let mut third = [0u8; 4];
    let first_count = read_into(&mut first, READ_TIMEOUT_NS).map_err(|_| "bounds-1")?;
    let second_count = read_into(&mut second, READ_TIMEOUT_NS).map_err(|_| "bounds-2")?;
    let third_count = read_into(&mut third, READ_TIMEOUT_NS).map_err(|_| "bounds-3")?;
    if first_count != 4 || &first != b"ABCD" {
        return Err("bounds-first");
    }
    if second_count != 4 || &second != b"EFGH" {
        return Err("bounds-second");
    }
    if third_count != 2 || &third[..2] != b"IJ" {
        return Err("bounds-third");
    }
    let mut extra = [0u8; 4];
    let extra_count = read_into(&mut extra, EMPTY_TIMEOUT_NS).map_err(|_| "bounds-extra")?;
    if extra_count != 0 {
        return Err("bounds-extra-count");
    }
    println!("console-input: bounds=4,4,2 extra=0");
    Ok(())
}

fn burst_row() -> Result<(), &'static str> {
    println!("console-input: ready-burst");
    sys::nanosleep(SETTLE_NS).map_err(|_| "burst-sleep")?;
    let mut received = [0u8; 64];
    let mut used = 0usize;
    while used < BURST.len() {
        let mut chunk = [0u8; 8];
        let count = read_into(&mut chunk, READ_TIMEOUT_NS).map_err(|_| "burst-read")?;
        if count == 0 {
            return Err("burst-timeout");
        }
        if count > chunk.len() || count > BURST.len().saturating_sub(used) {
            return Err("burst-overrun");
        }
        let Some(destination) = received.get_mut(used..used + count) else {
            return Err("burst-range");
        };
        destination.copy_from_slice(&chunk[..count]);
        used = used.saturating_add(count);
    }
    if &received != BURST {
        return Err("burst-bytes");
    }
    println!("console-input: burst=1 bytes=64");
    Ok(())
}

fn timeout_row() -> Result<(), &'static str> {
    println!("console-input: ready-timeout");
    let start = clock_ns().map_err(|_| "timeout-clock")?;
    let mut buffer = [0u8; 8];
    let count = read_into(&mut buffer, EMPTY_TIMEOUT_NS).map_err(|_| "timeout-read")?;
    let elapsed = clock_ns()
        .map_err(|_| "timeout-clock-after")?
        .saturating_sub(start);
    if count != 0 {
        return Err("timeout-count");
    }
    if elapsed < EMPTY_MIN_NS || elapsed > EMPTY_MAX_NS {
        return Err("timeout-window");
    }
    println!(
        "console-input: timeout=0 elapsed-ms={}",
        elapsed / 1_000_000
    );
    Ok(())
}

extern "C" fn signal_child(_arg: usize) -> ! {
    println!("console-input: ready-signal");
    let mut buffer = [0u8; 4];
    match read_into(&mut buffer, 0) {
        Err(Errno::EINTR) => println!("console-input: signal-eintr=1"),
        _ => {
            println!("console-input: fail signal-eintr");
            sys::exit(1);
        }
    }
    match sigrecv() {
        Ok(signo) if signo == SIGUSR1 => println!("console-input: signal-recv={SIGUSR1}"),
        _ => {
            println!("console-input: fail signal-recv");
            sys::exit(1);
        }
    }
    let count = match read_exact(&mut buffer, READ_TIMEOUT_NS * 2) {
        Ok(count) => count,
        Err(_) => {
            println!("console-input: fail signal-read");
            sys::exit(1);
        }
    };
    if count == buffer.len() && &buffer == b"TAIL" {
        println!("console-input: signal-tail=1");
        sys::exit(0);
    }
    println!("console-input: fail signal-tail");
    sys::exit(1);
}

fn signal_row() -> Result<(), &'static str> {
    println!("console-input: signal-start");
    let child = proc_fork(Handle::SELF_PROC).map_err(|_| "signal-fork")?;
    let pid = handle_info(child).map_err(|_| "signal-pid")?;
    let _thread = thread_create(child, signal_child as *const () as usize, stack_top(), 0)
        .map_err(|_| "signal-thread")?;
    sys::nanosleep(300_000_000).map_err(|_| "signal-sleep")?;
    signal(pid as i32, SIGUSR1).map_err(|_| "signal-send")?;
    let mut status = 0i32;
    wait_ex(P_PID, pid as i32, 0, &mut status).map_err(|_| "signal-wait")?;
    if (status >> 8) & 0xff != 0 {
        return Err("signal-child");
    }
    println!("console-input: signal-child=0");
    Ok(())
}

fn run() -> Result<(), &'static str> {
    zero_row()?;
    bad_pointer_row()?;
    bounds_row()?;
    burst_row()?;
    timeout_row()?;
    signal_row()?;
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("console-input: ok");
            0
        }
        Err(row) => {
            println!("console-input: fail {row}");
            1
        }
    }
}
