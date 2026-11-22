// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S106 aarch64 four-way concurrent unmap / TLB-shootdown regression.
//!
//! Three worker threads and the controller park distinct 16-page anonymous mappings, meet at a
//! barrier and unmap together. That produces four concurrent `flush_range` callers on four CPUs,
//! which is the waiter pattern the single-controller `smp-tlb-aarch64` case does not exercise.
//! After the barrier the controller remaps a shared object page while the three readers sample
//! the same virtual address; a stale TLB entry, or a lost shootdown acknowledgement, shows up as
//! a stale read or the kernel's `aarch64-smp: tlb shootdown timeout` line.
//!
//! The case forbids the timeout line and asserts `tlb_stale=0`. The fixture never weakens its
//! checks, never retries a failed round and never yields inside a reader's sampling loop: a
//! context switch would change TTBR0 and flush the TLB, which could hide a missing shootdown.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const WORKERS: usize = 3;
const WAITERS: usize = WORKERS + 1;
const MAP_PAGES: usize = 16;
const STACK_LEN: usize = 16 * PAGE;
const TLB_ROUNDS: u32 = 64;
/// Bounded handshake budget; this is a spin count, not a wall-clock claim.
const POLL_LIMIT: u32 = 20_000_000;
/// Raw kernel `pipe(fds)` number (kernel-owned, not a `Syscall` variant).
const PIPE: usize = 0x6e;

const VALUE_LOW: u64 = 0x1111_1111_1111_1111;
const VALUE_HIGH: u64 = 0x2222_2222_2222_2222;

/// Mapping each worker parks before the four-way unmap barrier.
static WAITER_ADDR: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
/// Worker has mapped and published its barrier mapping.
static WAITER_PREP: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
/// Worker finished its barrier unmap.
static WAITER_DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
/// Controller releases all four unmaps together.
static WAITER_GO: AtomicU32 = AtomicU32::new(0);
/// Worker survived the barrier and is ready to sample.
static READY: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];

/// Virtual address the readers sample (set once before the threads start).
static MAP_ADDR: AtomicUsize = AtomicUsize::new(0);
/// Remap round the controller has published.
static GO: AtomicU32 = AtomicU32::new(0);
/// Last round a reader finished.
static DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
/// Last value a reader observed.
static SEEN: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
/// Set by a reader just before it exits.
static WORKER_DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
/// Controller is done; the readers exit.
static STOP: AtomicU32 = AtomicU32::new(0);
/// A worker reported a failure.
static FAILED: AtomicU32 = AtomicU32::new(0);
/// Stale reads observed during the remap rounds.
static TLB_STALE: AtomicU32 = AtomicU32::new(0);

fn fail(message: &str) -> i32 {
    println!("smp-aarch64-tlb: fail {message}");
    1
}

fn worker_fail(index: usize, message: &str, error: Errno) -> ! {
    FAILED.store(1, Ordering::Release);
    println!(
        "smp-aarch64-tlb: worker {index} failed {message} errno={}",
        error.0
    );
    sys::thread_exit(1)
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

fn protect(address: usize, len: usize, protection: usize) -> Result<(), Errno> {
    // SAFETY: PROTECT takes scalar arguments; the kernel validates the range and protection.
    Errno::from_ret(unsafe {
        sys::syscall4(
            Syscall::PROTECT as usize,
            Handle::SELF_PROC.0 as usize,
            address,
            len,
            protection,
        )
    })
    .map(|_| ())
}

fn debug_write_raw(pointer: usize, length: usize) -> Result<usize, Errno> {
    // SAFETY: DEBUG_WRITE takes a user pointer and length; the kernel validates the pointer.
    Errno::from_ret(unsafe { sys::syscall2(Syscall::DEBUG_WRITE as usize, pointer, length) })
}

fn uname_raw(pointer: usize) -> Result<usize, Errno> {
    // SAFETY: UNAME writes a fixed utsname record to the user pointer; the kernel validates it.
    Errno::from_ret(unsafe { sys::syscall1(Syscall::UNAME as usize, pointer) })
}

fn clock_get_raw(clock_id: usize, pointer: usize) -> Result<usize, Errno> {
    // SAFETY: CLOCK_GET writes a timespec to the user pointer; the kernel validates it.
    Errno::from_ret(unsafe { sys::syscall2(Syscall::CLOCK_GET as usize, clock_id, pointer) })
}

fn pipe_create() -> Result<(Handle, Handle), Errno> {
    let mut fds = [0u32; 2];
    // SAFETY: PIPE writes two u32 handles into the live array.
    Errno::from_ret(unsafe { sys::syscall1(PIPE, fds.as_mut_ptr() as usize) })?;
    Ok((Handle(fds[0]), Handle(fds[1])))
}

fn write_raw(handle: Handle, pointer: usize, length: usize) -> Result<usize, Errno> {
    // SAFETY: WRITE takes a handle, user pointer and length; the kernel validates both.
    Errno::from_ret(unsafe {
        sys::syscall3(Syscall::WRITE as usize, handle.0 as usize, pointer, length)
    })
}

/// K461 vector class on aarch64: PROT_NONE/straddle reads and PROT_NONE copy-out must EFAULT.
fn usercopy_contracts() -> Result<(), Errno> {
    let none = sys::map_anonymous(PAGE)?;
    protect(none, PAGE, 0)?;
    let straddle = sys::map_anonymous(2usize.saturating_mul(PAGE))?;
    protect(straddle.saturating_add(PAGE), PAGE, 0)?;
    let boundary = straddle.saturating_add(PAGE).saturating_sub(8);
    let (pipe_r, pipe_w) = pipe_create()?;
    let mut next = [0u8; 512];

    let d_none = debug_write_raw(none, 1);
    let d_str = debug_write_raw(boundary, 16);
    let w_none = write_raw(pipe_w, none, 1);
    let w_str = write_raw(pipe_w, boundary, 16);
    let u_none = uname_raw(none);
    let c_none = clock_get_raw(clock::MONOTONIC as usize, none);
    let d_next = debug_write_raw(next.as_ptr() as usize, 1);
    let u_next = uname_raw(next.as_mut_ptr() as usize);
    let w_next = write_raw(pipe_w, next.as_ptr() as usize, 1);

    let _ = sys::handle_close(pipe_r);
    let _ = sys::handle_close(pipe_w);
    let _ = sys::unmap(Handle::SELF_PROC, none, PAGE);
    let _ = sys::unmap(Handle::SELF_PROC, straddle, 2usize.saturating_mul(PAGE));

    let d_none_err = d_none.as_ref().err().copied();
    let d_str_err = d_str.as_ref().err().copied();
    let w_none_err = w_none.as_ref().err().copied();
    let w_str_err = w_str.as_ref().err().copied();
    let u_none_err = u_none.as_ref().err().copied();
    let c_none_err = c_none.as_ref().err().copied();
    let d_next_ok = d_next.is_ok();
    let u_next_ok = u_next.is_ok();
    let w_next_ok = w_next.is_ok();
    let ok = d_none_err == Some(Errno::EFAULT)
        && d_str_err == Some(Errno::EFAULT)
        && w_none_err == Some(Errno::EFAULT)
        && w_str_err == Some(Errno::EFAULT)
        && u_none_err == Some(Errno::EFAULT)
        && c_none_err == Some(Errno::EFAULT)
        && d_next_ok
        && u_next_ok
        && w_next_ok;
    println!(
        "smp-aarch64-copy: protnone-debug={:?} straddle-debug={:?} protnone-write={:?} straddle-write={:?} protnone-uname={:?} protnone-clock={:?} next-debug={d_next_ok} next-uname={u_next_ok} next-write={w_next_ok}",
        d_none_err.map(|error| error.0),
        d_str_err.map(|error| error.0),
        w_none_err.map(|error| error.0),
        w_str_err.map(|error| error.0),
        u_none_err.map(|error| error.0),
        c_none_err.map(|error| error.0)
    );
    if ok {
        println!("smp-aarch64-copy: ok");
        Ok(())
    } else {
        println!("smp-aarch64-copy: fail want=14");
        Err(Errno::EIO)
    }
}

/// Sample one aligned word through a raw user address.
fn read_word(address: usize) -> u64 {
    // SAFETY: the caller passes an address inside the fixture's shared mapping; the GO/DONE
    // handshake keeps the controller from remapping it while a reader samples it.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write_word(address: usize, value: u64) {
    // SAFETY: as in `read_word`; the mapping is writable.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn write_page_byte(address: usize, value: u8) {
    // SAFETY: the caller passes an address inside a mapped page, one byte before the next page.
    unsafe {
        core::ptr::write_volatile(address as *mut u8, value);
    }
}

fn wait_flag(flag: &AtomicU32, what: &str) -> Result<(), Errno> {
    let mut spins = 0u32;
    while flag.load(Ordering::Acquire) == 0 {
        if FAILED.load(Ordering::Acquire) != 0 {
            println!("smp-aarch64-tlb: {what} observed worker failure");
            return Err(Errno::EIO);
        }
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            println!("smp-aarch64-tlb: {what} timeout");
            return Err(Errno::ETIME);
        }
    }
    Ok(())
}

fn wait_round(done: &AtomicU32, round: u32, what: &str) -> Result<(), Errno> {
    let mut spins = 0u32;
    while done.load(Ordering::Acquire) != round {
        if FAILED.load(Ordering::Acquire) != 0 {
            println!("smp-aarch64-tlb: {what} observed worker failure");
            return Err(Errno::EIO);
        }
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            println!("smp-aarch64-tlb: {what} timeout round={round}");
            return Err(Errno::ETIME);
        }
    }
    Ok(())
}

extern "C" fn worker(index: usize) -> ! {
    let len = MAP_PAGES.saturating_mul(PAGE);
    let base = match sys::map_anonymous(len) {
        Ok(base) => base,
        Err(error) => worker_fail(index, "barrier map", error),
    };
    // Touch every page so the unmap has something to shoot down on this CPU.
    let mut page = 0usize;
    while page < MAP_PAGES {
        write_page_byte(
            base.saturating_add(page.saturating_mul(PAGE)),
            (index as u8).wrapping_add(page as u8),
        );
        page = page.saturating_add(1);
    }
    WAITER_ADDR[index].store(base, Ordering::Release);
    WAITER_PREP[index].store(1, Ordering::Release);

    // Spin, do not yield: the coordinator needs the four unmaps to overlap.
    while WAITER_GO.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    if let Err(error) = unmap(base, len) {
        worker_fail(index, "barrier unmap", error);
    }
    WAITER_DONE[index].store(1, Ordering::Release);
    READY[index].store(1, Ordering::Release);

    let mut last = 0u32;
    loop {
        if STOP.load(Ordering::Acquire) != 0 {
            break;
        }
        let round = GO.load(Ordering::Acquire);
        if round == 0 || round == last {
            core::hint::spin_loop();
            continue;
        }
        let address = MAP_ADDR.load(Ordering::Acquire);
        SEEN[index].store(read_word(address), Ordering::Release);
        DONE[index].store(round, Ordering::Release);
        last = round;
    }
    WORKER_DONE[index].store(1, Ordering::Release);
    sys::thread_exit(0)
}

fn tlb_setup() -> Result<(Handle, usize, usize), Errno> {
    let object = memobj_create(2usize.saturating_mul(PAGE))?;
    let sampled = map_shared(object, 0, 0, 2usize.saturating_mul(PAGE), false)?;
    let keep = map_shared(object, 0, 0, 2usize.saturating_mul(PAGE), false)?;
    if sampled == keep {
        return Err(Errno::EINVAL);
    }
    write_word(sampled, VALUE_LOW);
    write_word(sampled.saturating_add(PAGE), VALUE_HIGH);
    MAP_ADDR.store(sampled, Ordering::Release);
    println!("smp-aarch64-tlb: map={sampled:#x} alias={keep:#x}");
    Ok((object, sampled, keep))
}

fn concurrent_unmap_barrier() -> Result<(), Errno> {
    for prep in &WAITER_PREP {
        wait_flag(prep, "waiter prepare")?;
    }
    let len = MAP_PAGES.saturating_mul(PAGE);
    let controller_map = sys::map_anonymous(len)?;
    let mut page = 0usize;
    while page < MAP_PAGES {
        write_page_byte(
            controller_map.saturating_add(page.saturating_mul(PAGE)),
            0x80u8.wrapping_add(page as u8),
        );
        page = page.saturating_add(1);
    }
    println!("smp-aarch64-tlb: concurrent-shootdown waiters={WAITERS}");
    WAITER_GO.store(1, Ordering::Release);
    unmap(controller_map, len)?;
    for done in &WAITER_DONE {
        wait_flag(done, "waiter unmap")?;
    }
    for ready in &READY {
        wait_flag(ready, "waiter ready")?;
    }
    Ok(())
}

fn remap_round(object: Handle, sampled: usize, round: u32) -> Result<(), Errno> {
    let (offset, want) = if round % 2 == 1 {
        (PAGE, VALUE_HIGH)
    } else {
        (0, VALUE_LOW)
    };
    unmap(sampled, PAGE)?;
    let address = map_shared(object, offset, sampled, PAGE, true)?;
    if address != sampled {
        return Err(Errno::EIO);
    }
    GO.store(round, Ordering::Release);
    let mut index = 0usize;
    while index < WORKERS {
        wait_round(&DONE[index], round, "reader sample")?;
        let observed = SEEN[index].load(Ordering::Acquire);
        if observed != want {
            TLB_STALE.fetch_add(1, Ordering::Relaxed);
            println!(
                "smp-aarch64-tlb: stale round={round} worker={index} seen={observed:#x} want={want:#x}"
            );
        }
        index = index.saturating_add(1);
    }
    Ok(())
}

fn stop_workers() -> Result<(), Errno> {
    STOP.store(1, Ordering::Release);
    GO.store(TLB_ROUNDS.saturating_add(1), Ordering::Release);
    for done in &WORKER_DONE {
        wait_flag(done, "worker join")?;
    }
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("smp-aarch64-tlb: start workers={WORKERS}");

    let (object, sampled, keep) = match tlb_setup() {
        Ok(setup) => setup,
        Err(error) => {
            println!("smp-aarch64-tlb: tlb setup failed errno={}", error.0);
            return 1;
        }
    };

    let mut stacks = [0usize; WORKERS];
    let mut index = 0usize;
    while index < WORKERS {
        let stack = match sys::map_anonymous(STACK_LEN) {
            Ok(stack) => stack,
            Err(error) => {
                println!("smp-aarch64-tlb: stack map failed errno={}", error.0);
                return 1;
            }
        };
        stacks[index] = stack.saturating_add(STACK_LEN);
        match sys::thread_create(
            Handle::SELF_PROC,
            worker as *const () as usize,
            stacks[index],
            index,
        ) {
            Ok(_) => {}
            Err(error) => {
                println!("smp-aarch64-tlb: thread create failed errno={}", error.0);
                return 1;
            }
        }
        index = index.saturating_add(1);
    }

    if let Err(error) = concurrent_unmap_barrier() {
        println!(
            "smp-aarch64-tlb: concurrent barrier failed errno={}",
            error.0
        );
        return 1;
    }
    println!("smp-aarch64-tlb: workers ready");

    let mut round = 1u32;
    while round <= TLB_ROUNDS {
        if let Err(error) = remap_round(object, sampled, round) {
            println!("smp-aarch64-tlb: round {round} failed errno={}", error.0);
            return 1;
        }
        round = round.saturating_add(1);
    }
    if let Err(error) = stop_workers() {
        println!("smp-aarch64-tlb: worker join failed errno={}", error.0);
        return 1;
    }

    if let Err(error) = usercopy_contracts() {
        println!("smp-aarch64-copy: failed errno={}", error.0);
        return fail("usercopy");
    }

    let _ = unmap(sampled, PAGE);
    let _ = unmap(keep, 2usize.saturating_mul(PAGE));
    let _ = sys::handle_close(object);

    let stale = TLB_STALE.load(Ordering::Acquire);
    println!(
        "smp-aarch64-tlb: summary waiters={WAITERS} tlb_rounds={TLB_ROUNDS} tlb_stale={stale}"
    );
    if stale != 0 || FAILED.load(Ordering::Acquire) != 0 {
        return fail("stress");
    }
    println!("smp-aarch64-tlb: ok");
    0
}
