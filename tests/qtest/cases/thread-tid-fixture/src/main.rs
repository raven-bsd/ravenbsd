// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S47 thread-tid fixture.
//!
//! Proves `THREAD_SELF`, the `THREAD_CREATE` `ThreadCreateEx` extension
//! (`tp`, `tid_out`, `clear_tid_addr`), clear-on-exit for plain `THREAD_EXIT`,
//! and `THREAD_EXIT_EX` clear + unmap.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, THREAD_EXIT_EX, THREAD_SELF, ThreadCreateEx};
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const TID_MAX: u32 = 0x3fff_ffff;

static SELF_TID: AtomicU32 = AtomicU32::new(0);
static TID_A: AtomicU32 = AtomicU32::new(0);
static TID_B: AtomicU32 = AtomicU32::new(0);
static A_SELF: AtomicU32 = AtomicU32::new(0);
static B_SELF: AtomicU32 = AtomicU32::new(0);
static CLEAR_A: AtomicU32 = AtomicU32::new(0xdead_beef);
static CLEAR_B: AtomicU32 = AtomicU32::new(0xfeed_face);
static A_DONE: AtomicBool = AtomicBool::new(false);
static B_DONE: AtomicBool = AtomicBool::new(false);
static B_STACK_BASE: AtomicU64 = AtomicU64::new(0);

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

fn self_tid() -> u32 {
    result(unsafe { syscall6(THREAD_SELF, 0, 0, 0, 0, 0, 0) })
        .map(|value| value as u32)
        .unwrap_or(0)
}

fn create_extended(
    entry: usize,
    stack_top: usize,
    arg: usize,
    extended: &ThreadCreateEx,
) -> Result<Handle, Errno> {
    result(unsafe {
        syscall6(
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

fn exit_ex(code: i32, clear_addr: usize, unmap_addr: usize, unmap_len: usize) -> isize {
    unsafe {
        syscall6(
            THREAD_EXIT_EX,
            code as usize,
            clear_addr,
            unmap_addr,
            unmap_len,
            0,
            0,
        )
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    controller()
}

fn controller() -> i32 {
    let tid = self_tid();
    SELF_TID.store(tid, Ordering::SeqCst);
    let self_ok = tid != 0 && tid <= TID_MAX;
    println!("thread-tid: self tid=0x{tid:x} ok={}", u8::from(self_ok));
    if !self_ok {
        return 1;
    }

    let a_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    CLEAR_A.store(0xdead_beef, Ordering::SeqCst);
    let a_ex = ThreadCreateEx {
        tp: 0,
        tid_out: &TID_A as *const _ as usize,
        clear_tid_addr: &CLEAR_A as *const _ as usize,
    };
    let _a = match create_extended(child_a as *const () as usize, a_stack + 2 * PAGE, 0, &a_ex) {
        Ok(handle) => handle,
        Err(_) => {
            println!("thread-tid: a create failed");
            return 1;
        }
    };
    println!("thread-tid: set-tid=1");
    let _ = sys::nanosleep(200_000_000);
    let tid_a = TID_A.load(Ordering::SeqCst);
    let a_match = A_SELF.load(Ordering::SeqCst) == tid_a && tid_a != 0 && tid_a <= TID_MAX;
    let a_clear = CLEAR_A.load(Ordering::SeqCst) == 0;
    let a_distinct = tid_a != tid;
    println!(
        "thread-tid: a self=0x{:x} expected=0x{:x} match={}",
        A_SELF.load(Ordering::SeqCst),
        tid_a,
        u8::from(a_match)
    );
    println!(
        "thread-tid: a exit clear={} distinct={} done={}",
        u8::from(a_clear),
        u8::from(a_distinct),
        u8::from(A_DONE.load(Ordering::SeqCst))
    );
    if !(a_match && a_clear && a_distinct) {
        return 1;
    }

    let bad = ThreadCreateEx {
        tp: 0,
        tid_out: 0,
        clear_tid_addr: 1,
    };
    match create_extended(child_a as *const () as usize, a_stack + 2 * PAGE, 0, &bad) {
        Err(Errno::EFAULT) => println!("thread-tid: bad=EFAULT"),
        _ => {
            println!("thread-tid: bad unexpected");
            return 1;
        }
    }

    let b_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    B_STACK_BASE.store(b_stack as u64, Ordering::SeqCst);
    CLEAR_B.store(0xfeed_face, Ordering::SeqCst);
    let b_ex = ThreadCreateEx {
        tp: 0,
        tid_out: &TID_B as *const _ as usize,
        clear_tid_addr: &CLEAR_B as *const _ as usize,
    };
    let _b = match create_extended(child_b as *const () as usize, b_stack + 2 * PAGE, 0, &b_ex) {
        Ok(handle) => handle,
        Err(_) => {
            println!("thread-tid: b create failed");
            return 1;
        }
    };
    let _ = sys::nanosleep(300_000_000);
    let tid_b = TID_B.load(Ordering::SeqCst);
    let b_match = B_SELF.load(Ordering::SeqCst) == tid_b && tid_b != 0 && tid_b <= TID_MAX;
    let b_clear = CLEAR_B.load(Ordering::SeqCst) == 0;
    let b_distinct = tid_b != tid && tid_b != tid_a;
    println!(
        "thread-tid: b self=0x{:x} expected=0x{:x} match={}",
        B_SELF.load(Ordering::SeqCst),
        tid_b,
        u8::from(b_match)
    );
    println!(
        "thread-tid: b exit clear={} distinct={} done={}",
        u8::from(b_clear),
        u8::from(b_distinct),
        u8::from(B_DONE.load(Ordering::SeqCst))
    );
    if !(b_match && b_clear && b_distinct) {
        return 1;
    }
    let unmap_ok = sys::unmap(Handle::SELF_PROC, b_stack, 2 * PAGE).is_err();
    println!("thread-tid: exit-ex unmap={}", u8::from(unmap_ok));
    if !unmap_ok {
        return 1;
    }

    println!("thread-tid: done");
    0
}

extern "C" fn child_a(_arg: usize) -> ! {
    let tid = self_tid();
    A_SELF.store(tid, Ordering::SeqCst);
    A_DONE.store(true, Ordering::SeqCst);
    println!(
        "thread-tid: a self=0x{tid:x} expected=0x{:x} match={}",
        TID_A.load(Ordering::SeqCst),
        u8::from(tid == TID_A.load(Ordering::SeqCst))
    );
    sys::thread_exit(0)
}

extern "C" fn child_b(_arg: usize) -> ! {
    let tid = self_tid();
    B_SELF.store(tid, Ordering::SeqCst);
    B_DONE.store(true, Ordering::SeqCst);
    println!(
        "thread-tid: b self=0x{tid:x} expected=0x{:x} match={}",
        TID_B.load(Ordering::SeqCst),
        u8::from(tid == TID_B.load(Ordering::SeqCst))
    );
    let raw = exit_ex(
        0,
        &CLEAR_B as *const _ as usize,
        B_STACK_BASE.load(Ordering::SeqCst) as usize,
        2 * PAGE,
    );
    println!("thread-tid: b exit-ex returned raw={raw}");
    loop {
        core::hint::spin_loop();
    }
}
