// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S76b aarch64 remote user TLB shootdown fixture.
//!
//! The controller (first user thread, CPU 0) maps a shared two-page memory object twice. The
//! reader thread (a later user thread, an application processor under `smp_user=on`) reads page
//! zero of the first mapping; the controller then unmaps that page and maps the object's *other*
//! page at the same virtual address. The two frames stay alive through the second mapping, so a
//! stale TLB entry on the reader's CPU would deterministically keep returning the previous
//! page's value while a correct invalidation returns the newly mapped one.
//!
//! Nothing in the fixture forces a CPU: the kernel's round-robin select-CPU policy places the
//! two threads on different CPUs, and the qtest asserts the kernel's `smp-user: cpu N first user
//! thread` trace. A lost shootdown either shows up as a stale value here or as the kernel's
//! `tlb shootdown timeout` line, which the case forbids.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_SELF};
use raven_abi::types::{Handle, clock, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;
use raven_smp_tlb::{StateSnapshot, deadline_reached};

entry!(main);

const PAGE: usize = 4096;
const OBJECT_PAGES: usize = 2;
const STACK_LEN: usize = 16 * PAGE;
/// Remap rounds. Each round is one unmap (which must shoot down) plus one fixed map.
const ROUNDS: u32 = 200;
/// Iterations between monotonic-clock reads. The reader must not yield (a context switch would
/// flush its TLB and hide a missing shootdown), so its clock check is periodic.
const CLOCK_CHECK_INTERVAL: u32 = 1 << 16;

const VALUE_LOW: u64 = 0x1111_1111_1111_1111;
const VALUE_HIGH: u64 = 0x2222_2222_2222_2222;

/// Virtual address the reader samples (set once before the thread starts).
static MAP_ADDR: AtomicUsize = AtomicUsize::new(0);
/// Round the controller has published for reading.
static GO: AtomicU32 = AtomicU32::new(0);
/// Round the reader finished reading.
static DONE: AtomicU32 = AtomicU32::new(0);
/// Last value the reader observed.
static SEEN: AtomicU64 = AtomicU64::new(0);
/// Set when the controller is done; the reader exits.
static STOP: AtomicU32 = AtomicU32::new(0);
/// Set by the reader just before it exits.
static EXITED: AtomicU32 = AtomicU32::new(0);
/// Reader thread id, so a timeout report names the thread the kernel placed on the AP.
static READER_TID: AtomicU32 = AtomicU32::new(0);

fn fail(message: &str) -> i32 {
    println!("smp-tlb: fail {message}");
    1
}

/// Nanoseconds from the monotonic clock, or `None` when the clock is unavailable.
fn now_ns() -> Option<u64> {
    let timespec = sys::clock_get(clock::MONOTONIC).ok()?;
    let seconds = u64::try_from(timespec.tv_sec).ok()?;
    let nanos = u64::try_from(timespec.tv_nsec).ok()?;
    Some(seconds.saturating_mul(1_000_000_000).saturating_add(nanos))
}

fn snapshot() -> StateSnapshot {
    StateSnapshot {
        go: GO.load(Ordering::Acquire),
        done: DONE.load(Ordering::Acquire),
        seen: SEEN.load(Ordering::Acquire),
        exited: EXITED.load(Ordering::Acquire),
        reader_tid: READER_TID.load(Ordering::Acquire),
    }
}

fn report_state(label: &str, round: u32) {
    println!("smp-tlb: {label} round={round} {}", snapshot());
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar arguments only; 0 selects the anonymous eager object.
    let value = Errno::from_ret(unsafe { sys::syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) })?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn map_shared(
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    fixed: bool,
) -> Result<usize, Errno> {
    let flags = if fixed { map::FIXED } else { 0 } | map::SHARED;
    sys::map_object_in(
        Handle::SELF_PROC,
        object,
        offset,
        address,
        len,
        prot::READ | prot::WRITE,
        flags,
    )
}

fn unmap(address: usize, len: usize) -> Result<(), Errno> {
    sys::unmap(Handle::SELF_PROC, address, len)
}

/// Sample one aligned word through a raw user address.
fn read_word(address: usize) -> u64 {
    // SAFETY: the caller passes an address inside the fixture's shared mapping; the controller
    // never remaps it while the reader is sampling (the GO/DONE handshake serialises them).
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write_word(address: usize, value: u64) {
    // SAFETY: as in `read_word`; the mapping is writable.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

extern "C" fn reader() -> ! {
    // SAFETY: THREAD_SELF takes no arguments and returns this thread's tid.
    let tid = unsafe { sys::syscall0(THREAD_SELF) };
    READER_TID.store(u32::try_from(tid).unwrap_or(0), Ordering::Release);
    let address = MAP_ADDR.load(Ordering::Acquire);
    let mut last = 0u32;
    loop {
        // Spin without yielding and without a syscall: a context switch (or a kernel entry that
        // swaps TTBR0) flushes this CPU's TLB, which would hide a missing shootdown. The
        // controller's wall-clock bound ends the process if the handshake never completes.
        while GO.load(Ordering::Acquire) == last {
            if STOP.load(Ordering::Acquire) != 0 {
                EXITED.store(1, Ordering::Release);
                sys::thread_exit(0)
            }
            core::hint::spin_loop();
        }
        let round = GO.load(Ordering::Acquire);
        SEEN.store(read_word(address), Ordering::Release);
        DONE.store(round, Ordering::Release);
        last = round;
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let object = match memobj_create(OBJECT_PAGES * PAGE) {
        Ok(object) => object,
        Err(_) => return fail("memobj create"),
    };
    // The sampled mapping. Its first page is remapped every round.
    let sampled = match map_shared(object, 0, 0, OBJECT_PAGES * PAGE, false) {
        Ok(address) => address,
        Err(_) => return fail("first map"),
    };
    // A second mapping of the same object keeps both frames mapped (and therefore allocated)
    // while the first mapping's page is replaced, so a stale entry keeps reading live memory.
    let keep = match map_shared(object, 0, 0, OBJECT_PAGES * PAGE, false) {
        Ok(address) => address,
        Err(_) => return fail("second map"),
    };
    if sampled == keep {
        return fail("alias");
    }
    write_word(sampled, VALUE_LOW);
    write_word(sampled.saturating_add(PAGE), VALUE_HIGH);
    if read_word(sampled) != VALUE_LOW || read_word(sampled.saturating_add(PAGE)) != VALUE_HIGH {
        return fail("seed");
    }
    println!("smp-tlb: map={sampled:#x} alias={keep:#x}");

    let stack = match sys::map_anonymous(STACK_LEN) {
        Ok(stack) => stack,
        Err(_) => return fail("stack map"),
    };
    MAP_ADDR.store(sampled, Ordering::Release);
    // The handle stays open for the process lifetime: the reader must not be reaped before the
    // final handshake.
    let _reader = match sys::thread_create(
        Handle::SELF_PROC,
        reader as *const () as usize,
        stack.saturating_add(STACK_LEN),
        0,
    ) {
        Ok(handle) => handle,
        Err(_) => return fail("thread create"),
    };

    let mut stale = 0u32;
    let mut round = 1u32;
    while round <= ROUNDS {
        // Odd rounds expose the high page, even rounds the low one: consecutive rounds always
        // differ, so a stale entry can never accidentally match the expected value.
        let (offset, want) = if round % 2 == 1 {
            (PAGE, VALUE_HIGH)
        } else {
            (0, VALUE_LOW)
        };
        if unmap(sampled, PAGE).is_err() {
            return fail("unmap");
        }
        match map_shared(object, offset, sampled, PAGE, true) {
            Ok(address) if address == sampled => {}
            _ => return fail("remap"),
        }
        GO.store(round, Ordering::Release);

        let Some(start) = now_ns() else {
            return fail("clock");
        };
        let mut yields = 0u32;
        while DONE.load(Ordering::Acquire) != round {
            if EXITED.load(Ordering::Acquire) != 0 {
                report_state("reader exit code", round);
                return fail("reader exit code");
            }
            sys::yield_now();
            yields = yields.wrapping_add(1);
            if yields & (CLOCK_CHECK_INTERVAL - 1) == 0 {
                let Some(now) = now_ns() else {
                    return fail("clock");
                };
                if deadline_reached(start, now) {
                    report_state("reader timeout", round);
                    return fail("reader timeout");
                }
            }
        }
        let seen = SEEN.load(Ordering::Acquire);
        if seen != want {
            stale = stale.saturating_add(1);
            println!("smp-tlb: stale round={round} seen={seen:#x} want={want:#x}");
        }
        round = round.saturating_add(1);
    }

    STOP.store(1, Ordering::Release);
    GO.store(ROUNDS.saturating_add(1), Ordering::Release);
    let Some(start) = now_ns() else {
        return fail("clock");
    };
    let mut yields = 0u32;
    while EXITED.load(Ordering::Acquire) == 0 {
        sys::yield_now();
        yields = yields.wrapping_add(1);
        if yields & (CLOCK_CHECK_INTERVAL - 1) == 0 {
            let Some(now) = now_ns() else {
                return fail("clock");
            };
            if deadline_reached(start, now) {
                report_state("reader join", ROUNDS);
                return fail("reader join");
            }
        }
    }
    if EXITED.load(Ordering::Acquire) != 1 {
        report_state("reader exit code", ROUNDS);
        return fail("reader exit code");
    }
    println!("smp-tlb: rounds={ROUNDS} stale={stale}");
    if stale != 0 {
        return fail("stale read");
    }
    println!("smp-tlb: ok");
    0
}
