// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S139 bounded setup-to-worker transition fixture.
//!
//! Starts `initfsd`, `ramfsd` and `vfsd`, runs the same setup step as the S85e stress fixture,
//! then creates the eight stress workers and waits for every one to enter its entry function.
//! The wait is wall-clock bounded so a KVM2 scheduling stall fails with the pending worker
//! indexes instead of being hidden by the cell cap.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, clock};
use raven_initfs::Initfs;
use raven_proto_fs::O_RDONLY;
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys;

entry!(main);

const WORKERS: usize = 8;
const START: &str = "/tmp/smp-start";
const STACK_SIZE: usize = 64 * 1024;
/// Headroom below the slot top for the entry frame and its buffers.
const STACK_GUARD: usize = 8 * 1024;
/// A worker that has not entered its entry function within this bound is reported as stuck.
const START_DEADLINE_NS: u64 = 10_000_000_000;

static GO: AtomicU32 = AtomicU32::new(0);
static STARTED: [AtomicBool; WORKERS] = [const { AtomicBool::new(false) }; WORKERS];

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// `static mut` keeps the stacks in writable `.bss`; an immutable `static` would be placed in
// `.rodata`, where `THREAD_CREATE` rejects the stack as non-writable.
static mut STACKS: [Stack; WORKERS] = [const { Stack([0; STACK_SIZE]) }; WORKERS];

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the stack memory is never referenced as a Rust value.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < WORKERS` and the array lives for the whole test, so `base.add(index)` stays
    // within the static; the result is only used as a thread stack pointer, never dereferenced.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE.saturating_sub(STACK_GUARD))
}

fn now_ns() -> u64 {
    let time = raven_rt::clock_get(clock::MONOTONIC).unwrap_or_default();
    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
    let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
    seconds.saturating_mul(1_000_000_000).saturating_add(nanos)
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if fs::open(scheme, O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = sys::nanosleep(10_000_000);
    }
    Err(format!("scheme {scheme} timeout"))
}

fn start_services() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    for path in ["/sbin/initfsd", "/sbin/ramfsd", "/sbin/vfsd"] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| format!("spawn {path}"))?;
        let scheme = match path {
            "/sbin/initfsd" => "initfs:",
            "/sbin/ramfsd" => "ramfs:",
            _ => "file:",
        };
        wait_scheme(scheme)?;
    }
    println!("smp-start: services ok");
    Ok(())
}

fn setup() -> Result<(), String> {
    match fs::mkdir(START, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(format!("mkdir {START}: {}", error.description())),
    }
    println!("smp-start: setup ok");
    Ok(())
}

extern "C" fn worker(index: usize) -> ! {
    STARTED[index].store(true, Ordering::Release);
    while GO.load(Ordering::Acquire) == 0 {
        let _ = sys::nanosleep(200_000);
    }
    sys::thread_exit(0);
}

fn pending_started() -> Vec<String> {
    STARTED
        .iter()
        .enumerate()
        .filter(|(_, flag)| !flag.load(Ordering::Acquire))
        .map(|(index, _)| format!("{index}"))
        .collect()
}

fn start_workers() -> Result<(), String> {
    for index in 0..WORKERS {
        sys::thread_create(
            Handle::SELF_PROC,
            worker as *const () as usize,
            stack_top(index),
            index,
        )
        .map_err(|error| format!("thread {index}: {}", error.description()))?;
    }
    let deadline = now_ns().saturating_add(START_DEADLINE_NS);
    loop {
        if STARTED.iter().all(|flag| flag.load(Ordering::Acquire)) {
            GO.store(1, Ordering::Release);
            return Ok(());
        }
        if now_ns() > deadline {
            let pending = pending_started();
            println!("smp-start: worker start timeout pending={pending:?}");
            return Err(format!("worker start timeout pending={pending:?}"));
        }
        let _ = sys::nanosleep(200_000);
    }
}

fn run() -> Result<(), String> {
    start_services()?;
    setup()?;
    start_workers()?;
    println!("smp-start: workers started workers={WORKERS}");
    println!("smp-start: done");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(message) => {
            println!("smp-start: failed {message}");
            1
        }
    }
}
