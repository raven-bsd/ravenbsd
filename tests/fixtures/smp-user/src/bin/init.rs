// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S75 x86-64 user SMP fixture.
//!
//! The controller starts four user threads. With `smp_user=on` the kernel's round-robin
//! select-CPU policy places them on distinct CPUs, and the kernel prints one `smp-user: cpu N
//! first user thread` line per CPU plus a `smp-user: wake` line when a futex wake targets
//! another CPU. The controller then proves a user fault and a process exit leave the rest of the
//! machine running.
//!
//! Roles (argv[1]): none (controller), `fault` (read an unmapped address), `exit` (exit 0).

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const WORKERS: usize = 4;
const ITERATIONS: usize = 4_000_000;
const STACK_SIZE: usize = 32 * 1024;
const FAULT_ADDRESS: usize = 0x3000_0000;

static STARTED: [AtomicBool; WORKERS] = [const { AtomicBool::new(false) }; WORKERS];
static FINISHED: [AtomicBool; WORKERS] = [const { AtomicBool::new(false) }; WORKERS];
static PROGRESS: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
static GATE: AtomicU32 = AtomicU32::new(0);

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// `static mut` keeps the stack array in writable `.bss`; an immutable `static` would be placed in
// `.rodata`, where the kernel's `THREAD_CREATE` rejects the stack as non-writable.
static mut STACKS: [Stack; WORKERS] = [const { Stack([0; STACK_SIZE]) }; WORKERS];

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the stack memory is never referenced as a Rust value.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < WORKERS` and the array lives for the whole test, so `base.add(index)` stays
    // within the static; the result is only used as a thread stack pointer, never dereferenced.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE)
}

fn fail(message: &str) -> i32 {
    println!("smp-user: failed {message}");
    1
}

fn map_initfs() -> Option<Initfs<'static>> {
    let image = raven_rt::map_initfs().ok()?;
    Initfs::parse(image).ok()
}

fn spawn_role(initfs: &Initfs<'_>, role: &str) -> Result<Handle, Errno> {
    raven_rt::spawn::spawn(
        initfs,
        "/init",
        &raven_rt::spawn::SpawnOptions {
            args: &[role, role],
            env: &[],
        },
    )
    .map_err(|_| Errno::EIO)
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT writes exactly one i32 to the live stack slot and validates the handle.
    Errno::from_ret(unsafe {
        sys::syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    })?;
    Ok(code)
}

fn thread_create(entry: usize, index: usize) -> Result<(), Errno> {
    let thread = sys::thread_create(Handle::SELF_PROC, entry, stack_top(index), index)?;
    // Keep the thread handle in the controller table so it stays alive across the test.
    let _ = thread;
    Ok(())
}

extern "C" fn worker(index: usize) -> ! {
    STARTED[index].store(true, Ordering::Release);
    while PROGRESS[index].load(Ordering::Relaxed) < ITERATIONS {
        PROGRESS[index].fetch_add(1, Ordering::Relaxed);
    }
    FINISHED[index].store(true, Ordering::Release);
    let _ = sys::futex_wait(&GATE, 0, None);
    sys::thread_exit(0)
}

fn controller(initfs: &Initfs<'_>) -> i32 {
    for index in 0..WORKERS {
        if let Err(error) = thread_create(worker as *const () as usize, index) {
            println!(
                "smp-user: thread create {index} errno={} entry={:#x} stack={:#x}",
                error.0,
                worker as *const () as usize,
                stack_top(index)
            );
            return fail("thread create");
        }
    }

    let mut spins = 0usize;
    while !STARTED.iter().all(|flag| flag.load(Ordering::Acquire)) {
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins > 1_000_000 {
            return fail("worker start");
        }
    }

    // Give the workers time to run their busy loops concurrently.
    let _ = sys::nanosleep(20_000_000);

    spins = 0;
    while !FINISHED.iter().all(|flag| flag.load(Ordering::Acquire)) {
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins > 10_000_000 {
            return fail("worker finish");
        }
    }

    println!(
        "smp-user: progress 0={} 1={} 2={} 3={}",
        PROGRESS[0].load(Ordering::Relaxed),
        PROGRESS[1].load(Ordering::Relaxed),
        PROGRESS[2].load(Ordering::Relaxed),
        PROGRESS[3].load(Ordering::Relaxed)
    );

    // Let the workers actually block on the gate, then wake them from (possibly) another CPU.
    let _ = sys::nanosleep(20_000_000);
    let _ = sys::futex_wake(&GATE, WORKERS as u32);

    // A child that faults must not corrupt this process or the other CPUs.
    match spawn_role(initfs, "fault") {
        Ok(child) => match proc_wait(child) {
            Ok(-11) => println!("smp-user: fault child reaped"),
            Ok(_code) => return fail("fault child code"),
            Err(_) => return fail("fault child wait"),
        },
        Err(_) => return fail("fault spawn"),
    }

    // A clean process exit must leave the rest of the machine running.
    match spawn_role(initfs, "exit") {
        Ok(child) => match proc_wait(child) {
            Ok(0) => println!("smp-user: exit child reaped"),
            Ok(_code) => return fail("exit child code"),
            Err(_) => return fail("exit child wait"),
        },
        Err(_) => return fail("exit spawn"),
    }

    println!("smp-user: done");
    0
}

fn fault_child() -> i32 {
    // SAFETY: the address is deliberately unmapped; the read faults and the kernel kills the
    // process. The value is discarded, so this only observes the fault side effect.
    let _ = unsafe { core::ptr::read_volatile(FAULT_ADDRESS as *const u8) };
    0
}

fn main(args: raven_rt::Args) -> i32 {
    if let Some(role) = args.get(1) {
        return match role {
            "fault" => fault_child(),
            "exit" => 0,
            _ => 2,
        };
    }
    let Some(initfs) = map_initfs() else {
        return fail("initfs map");
    };
    controller(&initfs)
}
