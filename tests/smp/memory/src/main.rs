// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S85b SMP memory, paging and TLB stress fixture.
//!
//! Three user threads and the controller run concurrent alloc/free and anonymous-map/fault
//! loops on four CPUs. The controller then remaps a shared page while every reader samples the
//! same address, forcing a cross-CPU TLB shootdown per round. Finally it forks two children at a
//! time; each child allocates, faults and exits while the parent waits.
//!
//! The fixture is deliberately independent of the allocator's internal state: every allocation is
//! filled and verified, every mapped page is written and read back, and any stale TLB read is
//! counted and reported as a failure.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const WORKERS: usize = 3;
const STACK_SIZE: usize = 64 * 1024;
const ALLOC_ROUNDS: usize = 64;
const MAP_ROUNDS: usize = 8;
const MAP_PAGES: usize = 16;
const TLB_ROUNDS: u32 = 64;
const FORK_ROUNDS: usize = 2;
const POLL_LIMIT: u32 = 50_000_000;
const PROC_FORK: usize = 0x39;

const VALUE_LOW: u64 = 0x1111_1111_1111_1111;
const VALUE_HIGH: u64 = 0x2222_2222_2222_2222;

static READY: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static SEEN: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static WORKER_DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static GO: AtomicU32 = AtomicU32::new(0);
static STOP: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicU32 = AtomicU32::new(0);
static MAP_ADDR: AtomicUsize = AtomicUsize::new(0);
static ALLOC_OPS: AtomicU64 = AtomicU64::new(0);
static FAULT_PAGES: AtomicU64 = AtomicU64::new(0);
static MAPS: AtomicU64 = AtomicU64::new(0);
static FORKS: AtomicU64 = AtomicU64::new(0);
static TLB_STALE: AtomicU32 = AtomicU32::new(0);

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// Writable `.bss`: a thread stack must be in a writable VMA.
static mut STACKS: [Stack; WORKERS] = [const { Stack([0; STACK_SIZE]) }; WORKERS];
static mut CHILD_STACK: Stack = Stack([0; STACK_SIZE]);
static mut CHILD_COW: [u8; PAGE] = [0; PAGE];

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the memory is used as a thread stack, never as a Rust
    // value in this address space.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < WORKERS` and the array lives for the whole test.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE)
}

fn child_stack_top() -> usize {
    let base = core::ptr::addr_of_mut!(CHILD_STACK);
    (base as usize).saturating_add(STACK_SIZE)
}

fn fail(message: &str) {
    println!("smp-memory: failed {message}");
}

fn worker_fail(index: usize, message: &str, error: Errno) -> ! {
    FAILED.store(1, Ordering::Release);
    println!(
        "smp-memory: worker {index} failed {message} errno={}",
        error.0
    );
    sys::thread_exit(1)
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar arguments; zero selects the anonymous eager object.
    let raw = unsafe { sys::syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) };
    Errno::from_ret(raw)
        .map(|value| Handle(value as u32))
        .map_err(|_| Errno::EINVAL)
}

fn write_word(address: usize, value: u64) {
    // SAFETY: the caller passes an address inside a writable mapping.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn read_word(address: usize) -> u64 {
    // SAFETY: the caller passes an address inside a readable mapping; the TLB handshake prevents
    // the controller from remapping it while a reader samples it.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

/// Concurrent allocator and anonymous-map pressure.
fn alloc_fault_rounds(label: usize) -> Result<(), Errno> {
    let mut round = 0usize;
    while round < ALLOC_ROUNDS {
        let size = 32usize.saturating_add(
            round
                .saturating_mul(37)
                .saturating_add(label.saturating_mul(13))
                % 512,
        );
        let fill = (round as u8).wrapping_add(label as u8);
        let mut bytes = Vec::with_capacity(size);
        bytes.resize(size, fill);
        if bytes.iter().any(|byte| *byte != fill) {
            return Err(Errno::EIO);
        }
        let boxed = Box::new([fill; 64]);
        if boxed[0] != fill || boxed[63] != fill {
            return Err(Errno::EIO);
        }
        ALLOC_OPS.fetch_add(1, Ordering::Relaxed);
        drop(bytes);
        drop(boxed);
        round = round.saturating_add(1);
    }

    let mut map_round = 0usize;
    while map_round < MAP_ROUNDS {
        let len = MAP_PAGES.saturating_mul(PAGE);
        let base = sys::map_anonymous(len)?;
        let mut page = 0usize;
        while page < MAP_PAGES {
            let address = base.saturating_add(page.saturating_mul(PAGE));
            // SAFETY: the kernel mapped `len` writable bytes at `base`.
            unsafe {
                (address as *mut u8).write_volatile((page as u8).wrapping_add(label as u8));
            }
            page = page.saturating_add(1);
        }
        page = 0;
        while page < MAP_PAGES {
            let address = base.saturating_add(page.saturating_mul(PAGE));
            // SAFETY: the page is inside the mapping and was just written.
            let observed = unsafe { core::ptr::read_volatile(address as *const u8) };
            if observed != (page as u8).wrapping_add(label as u8) {
                return Err(Errno::EIO);
            }
            page = page.saturating_add(1);
        }
        sys::unmap(Handle::SELF_PROC, base, len)?;
        FAULT_PAGES.fetch_add(MAP_PAGES as u64, Ordering::Relaxed);
        MAPS.fetch_add(1, Ordering::Relaxed);
        map_round = map_round.saturating_add(1);
    }
    Ok(())
}

extern "C" fn worker(index: usize) -> ! {
    if let Err(error) = alloc_fault_rounds(index) {
        worker_fail(index, "alloc/fault", error);
    }
    READY[index].store(1, Ordering::Release);

    let mut last = 0u32;
    while STOP.load(Ordering::Acquire) == 0 {
        let round = GO.load(Ordering::Acquire);
        if round == 0 || round == last {
            core::hint::spin_loop();
            continue;
        }
        let address = MAP_ADDR.load(Ordering::Acquire);
        let observed = read_word(address);
        SEEN[index].store(observed, Ordering::Release);
        DONE[index].store(round, Ordering::Release);
        last = round;
    }
    WORKER_DONE[index].store(1, Ordering::Release);
    sys::thread_exit(0)
}

fn thread_create(index: usize) -> Result<Handle, Errno> {
    sys::thread_create(
        Handle::SELF_PROC,
        worker as *const () as usize,
        stack_top(index),
        index,
    )
}

fn wait_workers_ready() -> Result<(), Errno> {
    for ready in &READY {
        let mut spins = 0u32;
        while ready.load(Ordering::Acquire) == 0 {
            if FAILED.load(Ordering::Acquire) != 0 {
                return Err(Errno::EIO);
            }
            sys::yield_now();
            spins = spins.saturating_add(1);
            if spins > POLL_LIMIT {
                return Err(Errno::ETIME);
            }
        }
    }
    Ok(())
}

fn tlb_setup() -> Result<(Handle, usize, usize), Errno> {
    let object = memobj_create(2usize.saturating_mul(PAGE))?;
    let sampled = sys::map_object_in(
        Handle::SELF_PROC,
        object,
        0,
        0,
        2usize.saturating_mul(PAGE),
        prot::READ | prot::WRITE,
        map::SHARED,
    )?;
    let keep = sys::map_object_in(
        Handle::SELF_PROC,
        object,
        0,
        0,
        2usize.saturating_mul(PAGE),
        prot::READ | prot::WRITE,
        map::SHARED,
    )?;
    if sampled == keep {
        return Err(Errno::EINVAL);
    }
    write_word(sampled, VALUE_LOW);
    write_word(sampled.saturating_add(PAGE), VALUE_HIGH);
    MAP_ADDR.store(sampled, Ordering::Release);
    println!("smp-memory: tlb map={sampled:#x} alias={keep:#x}");
    Ok((object, sampled, keep))
}

fn tlb_round(object: Handle, sampled: usize, round: u32) -> Result<(), Errno> {
    let (offset, want) = if round % 2 == 1 {
        (PAGE, VALUE_HIGH)
    } else {
        (0, VALUE_LOW)
    };
    sys::unmap(Handle::SELF_PROC, sampled, PAGE)?;
    let address = sys::map_object_in(
        Handle::SELF_PROC,
        object,
        offset,
        sampled,
        PAGE,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )?;
    if address != sampled {
        return Err(Errno::EIO);
    }
    GO.store(round, Ordering::Release);

    for index in 0..WORKERS {
        let mut spins = 0u32;
        while DONE[index].load(Ordering::Acquire) != round {
            sys::yield_now();
            spins = spins.saturating_add(1);
            if spins > POLL_LIMIT {
                return Err(Errno::ETIME);
            }
        }
        let observed = SEEN[index].load(Ordering::Acquire);
        if observed != want {
            TLB_STALE.fetch_add(1, Ordering::Relaxed);
            println!(
                "smp-memory: stale round={round} worker={index} seen={observed:#x} want={want:#x}"
            );
        }
    }
    Ok(())
}

fn stop_workers() -> Result<(), Errno> {
    STOP.store(1, Ordering::Release);
    GO.store(TLB_ROUNDS.saturating_add(1), Ordering::Release);
    for done in &WORKER_DONE {
        let mut spins = 0u32;
        while done.load(Ordering::Acquire) == 0 {
            sys::yield_now();
            spins = spins.saturating_add(1);
            if spins > POLL_LIMIT {
                return Err(Errno::ETIME);
            }
        }
    }
    Ok(())
}

fn proc_fork() -> Result<Handle, Errno> {
    // SAFETY: PROC_FORK is a scalar-only syscall; the raw number matches the other fixtures and
    // the kernel validates the self-process handle and zero flags.
    let raw = unsafe { sys::syscall6(PROC_FORK, Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0) };
    Errno::from_ret(raw).map(|value| Handle(value as u32))
}

fn proc_wait(child: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: PROC_WAIT writes one i32 into the live stack slot and validates the handle.
    Errno::from_ret(unsafe {
        sys::syscall2(
            Syscall::PROC_WAIT as usize,
            child.0 as usize,
            (&mut code as *mut i32) as usize,
        )
    })?;
    Ok(code)
}

extern "C" fn child_entry(seed: usize) -> ! {
    // Touch a COW page before allocating, so the child exercises both the static-data COW path
    // and the allocator's anonymous-map path.
    // SAFETY: this is the child's private COW copy of the static page.
    unsafe {
        let byte = core::ptr::addr_of_mut!(CHILD_COW)
            .cast::<u8>()
            .add(seed % PAGE);
        byte.write_volatile((seed & 0xFF) as u8);
        if byte.read_volatile() != (seed & 0xFF) as u8 {
            sys::exit(1);
        }
    }
    if alloc_fault_rounds(seed).is_err() {
        sys::exit(2);
    }
    sys::exit(0)
}

fn start_child(seed: usize) -> Result<Handle, Errno> {
    let child = proc_fork()?;
    sys::thread_create(
        child,
        child_entry as *const () as usize,
        child_stack_top(),
        seed,
    )?;
    Ok(child)
}

fn fork_exit_rounds() -> Result<(), Errno> {
    let mut round = 0usize;
    while round < FORK_ROUNDS {
        let first = start_child(0x40usize.saturating_add(round.saturating_mul(2)))?;
        let second = start_child(0x41usize.saturating_add(round.saturating_mul(2)))?;
        let first_code = proc_wait(first)?;
        let second_code = proc_wait(second)?;
        if first_code != 0 || second_code != 0 {
            return Err(Errno::EIO);
        }
        FORKS.fetch_add(2, Ordering::Relaxed);
        round = round.saturating_add(1);
    }
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("smp-memory: start workers={WORKERS}");

    let (object, sampled, keep) = match tlb_setup() {
        Ok(setup) => setup,
        Err(error) => {
            println!("smp-memory: tlb setup failed errno={}", error.0);
            return 1;
        }
    };

    for index in 0..WORKERS {
        if thread_create(index).is_err() {
            fail("thread create");
            return 1;
        }
    }

    if let Err(error) = alloc_fault_rounds(0x20) {
        println!(
            "smp-memory: controller alloc/fault failed errno={}",
            error.0
        );
        return 1;
    }
    if let Err(error) = wait_workers_ready() {
        println!("smp-memory: worker start failed errno={}", error.0);
        return 1;
    }
    println!("smp-memory: workers ready");

    let mut round = 1u32;
    while round <= TLB_ROUNDS {
        if let Err(error) = tlb_round(object, sampled, round) {
            println!("smp-memory: tlb round {round} failed errno={}", error.0);
            return 1;
        }
        round = round.saturating_add(1);
    }
    if let Err(error) = stop_workers() {
        println!("smp-memory: worker join failed errno={}", error.0);
        return 1;
    }

    if let Err(error) = fork_exit_rounds() {
        println!("smp-memory: fork/exit failed errno={}", error.0);
        return 1;
    }

    let _ = sys::unmap(Handle::SELF_PROC, sampled, PAGE);
    let _ = sys::unmap(Handle::SELF_PROC, keep, 2usize.saturating_mul(PAGE));
    let _ = sys::handle_close(object);

    let stale = TLB_STALE.load(Ordering::Acquire);
    println!(
        "smp-memory: summary alloc_ops={} fault_pages={} maps={} forks={} tlb_rounds={} tlb_stale={}",
        ALLOC_OPS.load(Ordering::Acquire),
        FAULT_PAGES.load(Ordering::Acquire),
        MAPS.load(Ordering::Acquire),
        FORKS.load(Ordering::Acquire),
        TLB_ROUNDS,
        stale
    );
    if stale != 0 || FAILED.load(Ordering::Acquire) != 0 {
        fail("stress");
        return 1;
    }
    println!("smp-memory: ok");
    0
}
