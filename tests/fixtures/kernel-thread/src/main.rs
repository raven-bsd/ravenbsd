// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S23 kernel-thread fixture.
//!
//! Reproduces the libc-threads fault at `0x31` without the libc harness: the main thread sets
//! its TLS base with `THREAD_SET_TLS` and immediately reads `%fs:0x31`. The syscall must install
//! `IA32_FS_BASE` before returning, not on the next context switch. A second thread is created
//! with `THREAD_CREATE_EX`, its own TLS base, and reads the same offset after the switch-in; the
//! kernel must install the per-thread base before entering ring 3.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, THREAD_SET_TLS, ThreadCreateEx};
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const MAIN_TLS: usize = 0x5000_0000;
const CHILD_TLS: usize = 0x5000_1000;
const STACK_ADDR: usize = 0x6000_0000;
const STACK_PAGES: usize = 8;
const STACK_LEN: usize = STACK_PAGES * PAGE;
const STACK_TOP: usize = STACK_ADDR + STACK_LEN;
const TLS_OFFSET: usize = 0x31;

const MAIN_MARKER: u64 = 0x5a5a_5a5a_5a5a_5a5a;
const CHILD_MARKER: u64 = 0xa5a5_a5a5_a5a5_a5a5;

static CHILD_DONE: AtomicU32 = AtomicU32::new(0);
static CHILD_VALUE: AtomicU64 = AtomicU64::new(0);
static CHILD_TID: AtomicU32 = AtomicU32::new(0);
static CHILD_CLEAR: AtomicU32 = AtomicU32::new(1);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global kernel_thread_child_entry
    .type kernel_thread_child_entry, @function
kernel_thread_child_entry:
    call kernel_thread_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size kernel_thread_child_entry, . - kernel_thread_child_entry
"#
);

unsafe extern "C" {
    fn kernel_thread_child_entry();
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

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn map_page(address: usize) -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user range.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            address,
            PAGE,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        )
    })
    .map(|_| ())
}

fn map_stack() -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user range.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR,
            STACK_LEN,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        )
    })
    .map(|_| ())
}

fn set_tls(base: usize) -> Result<(), Errno> {
    // SAFETY: the base is a mapped writable TLS page; the kernel validates the user address.
    result(unsafe {
        syscall6(
            THREAD_SET_TLS as usize,
            Handle::NONE.0 as usize,
            base,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn create_child(tp: usize) -> Result<Handle, Errno> {
    let extended = ThreadCreateEx {
        tp,
        tid_out: &CHILD_TID as *const AtomicU32 as usize,
        clear_tid_addr: &CHILD_CLEAR as *const AtomicU32 as usize,
    };
    // SAFETY: `extended` is a live payload and the kernel validates every address.
    result(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            kernel_thread_child_entry as *const () as usize,
            STACK_TOP,
            0,
            THREAD_CREATE_EX,
            &extended as *const ThreadCreateEx as usize,
        )
    })
    .map(|value| Handle(value as u32))
}

fn read_tls_offset() -> u64 {
    let value: u64;
    // SAFETY: the caller installed a TLS base whose mapped page has a readable word at the
    // offset; the segment-base access is the point of the test.
    unsafe {
        asm!(
            "mov {}, qword ptr fs:[0x31]",
            out(reg) value,
            options(nostack, preserves_flags),
        );
    }
    value
}

fn write_marker(page: usize, marker: u64) {
    // SAFETY: `page` is a mapped writable page; the write is deliberately unaligned to use the
    // same 0x31 offset as the libc TLS access.
    unsafe {
        core::ptr::write_unaligned((page + TLS_OFFSET) as *mut u64, marker);
        core::ptr::write_unaligned(page as *mut u64, page as u64);
    }
}

fn yield_now() {
    // SAFETY: YIELD takes no arguments.
    let _ = unsafe { syscall6(Syscall::YIELD as usize, 0, 0, 0, 0, 0, 0) };
}

/// Child body: read the TLS word the kernel installed for this thread.
#[unsafe(no_mangle)]
pub extern "C" fn kernel_thread_child_body() -> i32 {
    CHILD_VALUE.store(read_tls_offset(), Ordering::Release);
    CHILD_DONE.store(1, Ordering::Release);
    0
}

fn main(_args: raven_rt::Args) -> i32 {
    if map_page(MAIN_TLS).is_err() || map_page(CHILD_TLS).is_err() || map_stack().is_err() {
        println!("kernel-thread: fail map");
        return 1;
    }
    write_marker(MAIN_TLS, MAIN_MARKER);
    write_marker(CHILD_TLS, CHILD_MARKER);

    // Immediate TLS: this read faults at 0x31 if the kernel defers FS_BASE to a context switch.
    if set_tls(MAIN_TLS).is_err() {
        println!("kernel-thread: fail set-tls");
        return 1;
    }
    if read_tls_offset() != MAIN_MARKER {
        println!("kernel-thread: fail main tls");
        return 1;
    }

    if create_child(CHILD_TLS).is_err() {
        println!("kernel-thread: fail create");
        return 1;
    }
    while CHILD_DONE.load(Ordering::Acquire) == 0 {
        yield_now();
    }
    if CHILD_VALUE.load(Ordering::Acquire) != CHILD_MARKER {
        println!("kernel-thread: fail child tls");
        return 1;
    }
    if CHILD_TID.load(Ordering::Acquire) == 0 {
        println!("kernel-thread: fail child tid");
        return 1;
    }
    println!("kernel-thread: ok");
    0
}
