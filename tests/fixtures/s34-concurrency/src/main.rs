// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S34 concurrency fixture.
//!
//! * C9: a one-shot timer registered under a different event mask is probed by `EVQ_WAIT`;
//!   the probe must not consume the period, so the subsequent `SIGNALED` wait still fires.
//! * C2: create/close event queues while timer ticks run `notify_all`, then wait on a periodic
//!   timer plus a user-event mix.
//! * C4: map/unmap memory objects to run TLB shootdowns; the `s34-tlb-fail` boot flag makes the
//!   first shootdown miss one acknowledgement and exercise the timeout path.
//! * Frame accounting: a child exits before and after the stress, and the kernel prints the free
//!   frame count (`s21-framecheck`) for both exits.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const INITFS_BASE: usize = 0x1000_0000;
const STACK_SCRATCH: usize = 0x2000_0000;
const STACK_SIZE: usize = 64 * 1024;
const STACK_TOP: usize = (USER_END as usize) - PAGE;
const STACK_BASE: usize = STACK_TOP - STACK_SIZE;

const READABLE: usize = 1;
const SIGNALED: usize = 8;
const EVQ_ADD: usize = 1;
const EVQ_MOD: usize = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct EvqEvent {
    cookie: u64,
    events: u32,
    pad: u32,
}

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
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates every
    // pointer and handle.
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

fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn fail(message: &str) -> ! {
    println!("s34: failed {message}");
    exit(1)
}

fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

fn handle_close(handle: Handle) {
    let _ = syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize);
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
}

fn map_memory(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            offset,
            address,
            len,
            flags,
        )
    })
}

fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::UNMAP as usize,
        process.0 as usize,
        address,
        len,
    ))
    .map(|_| ())
}

fn evq_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::EVQ_CREATE as usize, 0))
}

fn evq_ctl(
    queue: Handle,
    operation: usize,
    source: Handle,
    events: usize,
    cookie: u64,
) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::EVQ_CTL as usize,
            queue.0 as usize,
            operation,
            source.0 as usize,
            events,
            cookie as usize,
            0,
        )
    })
    .map(|_| ())
}

fn evq_wait(queue: Handle, event: &mut EvqEvent, timeout_ns: u64) -> Result<usize, Errno> {
    result(unsafe {
        syscall6(
            Syscall::EVQ_WAIT as usize,
            queue.0 as usize,
            event as *mut EvqEvent as usize,
            1,
            timeout_ns as usize,
            0,
            0,
        )
    })
}

fn event_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::EVENT_CREATE as usize, 0))
}

fn event_signal(event: Handle) -> Result<(), Errno> {
    result(syscall2(
        Syscall::EVENT_SIGNAL as usize,
        event.0 as usize,
        0,
    ))
    .map(|_| ())
}

fn timer_create(clock_id: u32) -> Result<Handle, Errno> {
    handle(syscall2(
        Syscall::TIMER_CREATE as usize,
        clock_id as usize,
        0,
    ))
}

fn timer_set(timer: Handle, initial_ns: u64, interval_ns: u64) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::TIMER_SET as usize,
            timer.0 as usize,
            initial_ns as usize,
            interval_ns as usize,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn proc_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::PROC_CREATE as usize, 0))
}

fn thread_create(process: Handle, entry: usize, stack: usize) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack,
            0,
            0,
            0,
        )
    })
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    result(syscall2(
        Syscall::PROC_WAIT as usize,
        process.0 as usize,
        &mut code as *mut i32 as usize,
    ))?;
    Ok(code)
}

fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ as usize,
    )?;
    // SAFETY: the mapping is one readable page; the initfs header stores total_size at byte 40.
    let total = unsafe { ((page + 40) as *const u64).read_unaligned() as usize };
    let _ = unmap(Handle::SELF_PROC, page, PAGE);
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the kernel mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

/// Spawn this binary with `argv[1] = "child"`; the child exits immediately.
fn spawn_child(initfs: &Initfs<'_>) -> Result<Handle, Errno> {
    let entry = initfs.get("/init").ok_or(Errno::ENOENT)?;
    let elf =
        Elf::parse(entry.data, Machine::X86_64, USER_MIN..USER_END).map_err(|_| Errno::EINVAL)?;
    let child = proc_create()?;
    let steps = plan_load(&elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL)?;
    for step in steps {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                map_memory(
                    child,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::Zero => {
                map_memory(
                    child,
                    Handle::NONE,
                    0,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = memobj_create(STACK_SIZE)?;
    let scratch = map_memory(
        Handle::SELF_PROC,
        stack,
        0,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    map_memory(
        child,
        stack,
        0,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    let stack_pointer = build_stack(scratch);
    let thread = thread_create(child, elf.entry() as usize, stack_pointer)?;
    handle_close(thread);
    unmap(Handle::SELF_PROC, STACK_SCRATCH, STACK_SIZE)?;
    handle_close(stack);
    Ok(child)
}

fn build_stack(base: usize) -> usize {
    let stack = STACK_TOP - 0x100;
    let argv0 = STACK_TOP - 0x40;
    let argv1 = argv0 + 2;
    let mut offset = stack - STACK_BASE;
    let words = [
        2u64,
        argv0 as u64,
        argv1 as u64,
        0u64,
        0u64,
        6u64,
        PAGE as u64,
        0u64,
        0u64,
    ];
    for value in words {
        // SAFETY: the shared stack mapping is writable and `offset` stays inside its fixed size.
        unsafe { ((base + offset) as *mut u64).write_unaligned(value.to_le()) };
        offset += 8;
    }
    // SAFETY: the strings are inside the writable shared stack mapping.
    unsafe {
        ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'x');
        ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0);
        let role = b"child";
        let start = base + STACK_TOP - STACK_BASE - 0x40 + 2;
        let mut index = 0usize;
        while index < role.len() {
            ((start + index) as *mut u8).write(*role.get(index).unwrap_or(&0));
            index = index.saturating_add(1);
        }
        ((start + role.len()) as *mut u8).write(0);
    }
    stack
}

/// C9: a readiness probe with a non-matching mask must not consume a one-shot period.
fn timer_probe() -> Result<(), &'static str> {
    let queue = evq_create().map_err(|_| "probe queue")?;
    let timer = timer_create(clock::MONOTONIC).map_err(|_| "probe timer")?;
    evq_ctl(queue, EVQ_ADD, timer, READABLE, 0xC9).map_err(|_| "probe add")?;
    timer_set(timer, 10_000_000, 0).map_err(|_| "probe arm")?;
    nanosleep(30_000_000);
    let mut event = EvqEvent {
        cookie: 0,
        events: 0,
        pad: 0,
    };
    if evq_wait(queue, &mut event, 1_000_000).map_err(|_| "probe wait")? != 0 {
        return Err("probe delivered");
    }
    evq_ctl(queue, EVQ_MOD, timer, SIGNALED, 0xC9).map_err(|_| "probe mod")?;
    if evq_wait(queue, &mut event, 100_000_000).map_err(|_| "timer wait")? != 1 {
        return Err("timer period consumed by probe");
    }
    if event.cookie != 0xC9 || event.events != SIGNALED as u32 {
        return Err("timer record");
    }
    handle_close(queue);
    handle_close(timer);
    println!("s34: timer probe ok");
    Ok(())
}

/// C2: queue registration/handle close against the timer tick, then a timer/event mix.
fn event_stress() -> Result<(), &'static str> {
    let mut round = 0usize;
    while round < 200 {
        let queue = evq_create().map_err(|_| "stress queue")?;
        let event = event_create().map_err(|_| "stress event")?;
        evq_ctl(queue, EVQ_ADD, event, SIGNALED, 0xE2).map_err(|_| "stress add")?;
        event_signal(event).map_err(|_| "stress signal")?;
        let mut record = EvqEvent {
            cookie: 0,
            events: 0,
            pad: 0,
        };
        if evq_wait(queue, &mut record, 1_000_000).map_err(|_| "stress wait")? != 1 {
            return Err("stress event lost");
        }
        handle_close(queue);
        handle_close(event);
        round = round.saturating_add(1);
    }
    let queue = evq_create().map_err(|_| "mix queue")?;
    let timer = timer_create(clock::MONOTONIC).map_err(|_| "mix timer")?;
    evq_ctl(queue, EVQ_ADD, timer, SIGNALED, 0xE3).map_err(|_| "mix add")?;
    timer_set(timer, 1_000_000, 1_000_000).map_err(|_| "mix arm")?;
    let mut count = 0usize;
    let mut waits = 0usize;
    while count < 20 && waits < 2_000 {
        let mut record = EvqEvent {
            cookie: 0,
            events: 0,
            pad: 0,
        };
        count = count.saturating_add(evq_wait(queue, &mut record, 50_000_000).unwrap_or(0));
        waits = waits.saturating_add(1);
    }
    if count < 20 {
        return Err("timer events lost");
    }
    handle_close(queue);
    handle_close(timer);
    println!("s34: event stress ok");
    Ok(())
}

/// C4: map/unmap memory objects to run shootdowns; the boot flag injects one lost ack.
fn shootdown_stress() -> Result<(), &'static str> {
    let len = 1 << 20;
    let mut round = 0usize;
    while round < 32 {
        let object = memobj_create(len).map_err(|_| "tlb object")?;
        let address = map_memory(
            Handle::SELF_PROC,
            object,
            0,
            0,
            len,
            map::pack(prot::READ | prot::WRITE, 0),
        )
        .map_err(|_| "tlb map")?;
        unmap(Handle::SELF_PROC, address, len).map_err(|_| "tlb unmap")?;
        handle_close(object);
        round = round.saturating_add(1);
    }
    println!("s34: shootdown stress ok");
    Ok(())
}

fn main(args: raven_rt::Args) -> i32 {
    if args.get(1) == Some("child") {
        return 0;
    }
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail("initfs parse"),
    };

    // Frame-count baseline: the kernel prints `s21: frames free=` on child exit.
    let before = match spawn_child(&initfs) {
        Ok(child) => child,
        Err(_) => fail("spawn before"),
    };
    if proc_wait(before).unwrap_or(-1) != 0 {
        fail("child before");
    }
    handle_close(before);
    println!("s34: frames before");

    if let Err(message) = timer_probe() {
        fail(message);
    }
    if let Err(message) = event_stress() {
        fail(message);
    }
    if let Err(message) = shootdown_stress() {
        fail(message);
    }

    let after = match spawn_child(&initfs) {
        Ok(child) => child,
        Err(_) => fail("spawn after"),
    };
    if proc_wait(after).unwrap_or(-1) != 0 {
        fail("child after");
    }
    handle_close(after);
    println!("s34: frames after");

    println!("s34: concurrency ok");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
