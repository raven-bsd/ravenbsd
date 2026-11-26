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
use raven_abi::types::{Handle, clock, map, prot};
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
/// Raw kernel `pipe(fds)` number (kernel-owned, not a `Syscall` variant).
const PIPE: usize = 0x6e;

/// End of the canonical user window (the same value as `raven-abi` layout).
const USER_END: usize = 0x0000_7FFF_FFFF_F000;
/// Largest eager object `MEMOBJ_CREATE` accepts; a request this size exceeds the guest's frames.
const OOM_OBJECT_BYTES: usize = 1 << 30;
/// Stack growth within the kernel's 256 KiB user stack.
const STACK_WITHIN: usize = 200 * 1024;
/// A depth below the 256 KiB user stack, past its guard.
const STACK_BEYOND: usize = 300 * 1024;

/// One OOM-injection chunk: 1 MiB, small enough to keep the VMA count bounded.
const OOM_CHUNK_PAGES: usize = 256;
const OOM_CHUNK_BYTES: usize = OOM_CHUNK_PAGES * PAGE;
/// The small first chunk each child maps before the shared start barrier.
const OOM_SMALL_PAGES: usize = 16;
/// More 1 MiB chunks than the guest can hold; the allocator must refuse before this.
const OOM_MAX_CHUNKS: usize = 640;
const OOM_REFUSALS: u32 = 3;

/// Per-child chunk list; the child process has a private copy of these statics.
static OOM_CHUNK_ADDR: [AtomicUsize; OOM_MAX_CHUNKS] =
    [const { AtomicUsize::new(0) }; OOM_MAX_CHUNKS];
static OOM_CHUNK_PAGES_USED: [AtomicUsize; OOM_MAX_CHUNKS] =
    [const { AtomicUsize::new(0) }; OOM_MAX_CHUNKS];

const VALUE_LOW: u64 = 0x1111_1111_1111_1111;
const VALUE_HIGH: u64 = 0x2222_2222_2222_2222;

static READY: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static SEEN: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static WORKER_DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static WAITER_PREP: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static WAITER_DONE: [AtomicU32; WORKERS] = [const { AtomicU32::new(0) }; WORKERS];
static WAITER_ADDR: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
static WAITER_GO: AtomicU32 = AtomicU32::new(0);
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

    // Park a 16-page anonymous mapping and wait for the controller's barrier. All four threads
    // unmap together, forcing four concurrent flush_range callers (the regression this suite
    // exists to catch).
    let barrier_len = MAP_PAGES.saturating_mul(PAGE);
    let barrier_base = match sys::map_anonymous(barrier_len) {
        Ok(base) => base,
        Err(error) => worker_fail(index, "shootdown-waiter map", error),
    };
    WAITER_ADDR[index].store(barrier_base, Ordering::Release);
    WAITER_PREP[index].store(1, Ordering::Release);
    while WAITER_GO.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    if let Err(error) = sys::unmap(Handle::SELF_PROC, barrier_base, barrier_len) {
        worker_fail(index, "shootdown-waiter unmap", error);
    }
    WAITER_DONE[index].store(1, Ordering::Release);
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

fn wait_worker_flags(flags: &[AtomicU32; WORKERS]) -> Result<(), Errno> {
    for flag in flags {
        let mut spins = 0u32;
        while flag.load(Ordering::Acquire) == 0 {
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

fn wait_workers_ready() -> Result<(), Errno> {
    wait_worker_flags(&READY)
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

// ---------------------------------------------------------------------------
// Memory contract rows (K453). Each row prints CONTRACT Kx PASS|FAIL and the
// contract suite fails the fixture if any row is red.
// ---------------------------------------------------------------------------

fn protect(process: Handle, address: usize, len: usize, protection: usize) -> Result<(), Errno> {
    // SAFETY: PROTECT takes scalar arguments; the kernel validates the range and protection.
    Errno::from_ret(unsafe {
        sys::syscall4(
            Syscall::PROTECT as usize,
            process.0 as usize,
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

fn memobj_create_raw(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar arguments; zero selects an eager anonymous object.
    Errno::from_ret(unsafe { sys::syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) })
        .map(|value| Handle(value as u32))
}

fn pipe_create() -> Result<(Handle, Handle), Errno> {
    let mut fds = [0u32; 2];
    // SAFETY: PIPE writes two u32 handles into the live array.
    Errno::from_ret(unsafe { sys::syscall1(PIPE, fds.as_mut_ptr() as usize) })?;
    Ok((Handle(fds[0]), Handle(fds[1])))
}

fn current_sp() -> usize {
    let sp: usize;
    // SAFETY: reading RSP does not dereference memory and does not modify the stack.
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) sp, options(nomem, nostack, preserves_flags));
    }
    sp
}

fn start_child_with(entry: usize, stack_top: usize, arg: usize) -> Result<Handle, Errno> {
    let child = proc_fork()?;
    sys::thread_create(child, entry, stack_top, arg)?;
    Ok(child)
}

fn wait_child(child: Handle) -> Result<i32, Errno> {
    let code = proc_wait(child)?;
    // The child's `s24-framecheck` line is printed after `PROC_WAIT` can observe Exited; let the
    // dying CPU finish the evidence line before the parent prints its contract line.
    let _ = sys::nanosleep(20_000_000);
    Ok(code)
}

extern "C" fn cow_child_entry(value: usize) -> ! {
    // SAFETY: this is the child's private COW copy of the static page.
    unsafe {
        core::ptr::addr_of_mut!(CHILD_COW)
            .cast::<u8>()
            .write_volatile((value & 0xFF) as u8);
    }
    sys::exit(0)
}

extern "C" fn fault_write_entry(address: usize) -> ! {
    // SAFETY: the parent passes an address that is unmapped or not writable in this child.
    unsafe {
        (address as *mut u8).write_volatile(0x5A);
    }
    // If the access succeeded, the contract failed; exit with a non-signal code.
    sys::exit(1)
}

extern "C" fn fault_read_entry(address: usize) -> ! {
    // SAFETY: the parent passes an address that is unmapped or not readable in this child.
    unsafe {
        let _ = (address as *const u8).read_volatile();
    }
    sys::exit(1)
}

extern "C" fn stack_beyond_entry(stack_top: usize) -> ! {
    let address = stack_top.saturating_sub(STACK_BEYOND);
    // SAFETY: the parent requests an access below the user stack guard; it must fault.
    unsafe {
        (address as *mut u8).write_volatile(0x5A);
    }
    sys::exit(1)
}

extern "C" fn oom_child_entry(_arg: usize) -> ! {
    let large = match memobj_create_raw(OOM_OBJECT_BYTES) {
        Ok(handle) => {
            let _ = sys::handle_close(handle);
            sys::exit(2)
        }
        Err(error) if error == Errno::ENOMEM => error,
        Err(_) => sys::exit(4),
    };
    let _ = large;
    // The pressure must be released: a normal object after the failed one must succeed.
    match memobj_create_raw(PAGE) {
        Ok(handle) => {
            let _ = sys::handle_close(handle);
            sys::exit(0)
        }
        Err(_) => sys::exit(3),
    }
}

fn contract_fault_unmapped() -> Result<(), Errno> {
    let address = 0x0000_5000_0000_0000usize;
    let child = match start_child_with(
        fault_write_entry as *const () as usize,
        child_stack_top(),
        address,
    ) {
        Ok(child) => child,
        Err(error) => {
            println!("CONTRACT K1 FAIL setup errno={}", error.0);
            return Err(error);
        }
    };
    let code = wait_child(child)?;
    if code == -11 {
        println!("CONTRACT K1 PASS unmapped=0x{address:x} status={code}");
        Ok(())
    } else {
        println!("CONTRACT K1 FAIL unmapped=0x{address:x} status={code} want=-11");
        Err(Errno::EIO)
    }
}

fn contract_protection() -> Result<(), Errno> {
    let page = sys::map_anonymous(PAGE)?;
    if let Err(error) = protect(Handle::SELF_PROC, page, PAGE, prot::READ) {
        println!("CONTRACT K2 FAIL protect-read errno={}", error.0);
        return Err(error);
    }
    let child = start_child_with(
        fault_write_entry as *const () as usize,
        child_stack_top(),
        page,
    )?;
    let code = wait_child(child)?;
    let _ = sys::unmap(Handle::SELF_PROC, page, PAGE);
    if code != -11 {
        println!("CONTRACT K2 FAIL ro-write status={code} want=-11");
        return Err(Errno::EIO);
    }

    let none = sys::map_anonymous(PAGE)?;
    if let Err(error) = protect(Handle::SELF_PROC, none, PAGE, 0) {
        println!("CONTRACT K2 FAIL protect-none errno={}", error.0);
        return Err(error);
    }
    let child = start_child_with(
        fault_read_entry as *const () as usize,
        child_stack_top(),
        none,
    )?;
    let code = wait_child(child)?;
    let _ = sys::unmap(Handle::SELF_PROC, none, PAGE);
    if code == -11 {
        println!("CONTRACT K2 PASS ro-write={code} none-read={code}");
        Ok(())
    } else {
        println!("CONTRACT K2 FAIL none-read status={code} want=-11");
        Err(Errno::EIO)
    }
}

fn contract_cow() -> Result<(), Errno> {
    // Fill the parent's COW page with a known pattern.
    // SAFETY: the static page is writable in the fixture's data VMA.
    unsafe {
        let base = core::ptr::addr_of_mut!(CHILD_COW).cast::<u8>();
        let mut index = 0usize;
        while index < PAGE {
            base.add(index).write_volatile(0x11);
            index = index.saturating_add(1);
        }
    }
    let child = start_child_with(
        cow_child_entry as *const () as usize,
        child_stack_top(),
        0x22,
    )?;
    let code = wait_child(child)?;
    // SAFETY: the static page is readable in the fixture's data VMA.
    let first = unsafe { core::ptr::addr_of!(CHILD_COW).cast::<u8>().read_volatile() };
    // SAFETY: the static page is readable in the fixture's data VMA.
    let last = unsafe {
        core::ptr::addr_of!(CHILD_COW)
            .cast::<u8>()
            .add(PAGE - 1)
            .read_volatile()
    };
    let cow = core::ptr::addr_of!(CHILD_COW) as usize;
    if code == 0 && first == 0x11 && last == 0x11 {
        println!("CONTRACT K3 PASS parent=0x11 child=0x22 status={code} cow=0x{cow:x}");
        Ok(())
    } else {
        println!(
            "CONTRACT K3 FAIL status={code} parent0={first:#x} parentN={last:#x} want=0x11 cow=0x{cow:x}"
        );
        Err(Errno::EIO)
    }
}

fn contract_mmap_errno() -> Result<(), Errno> {
    let page = sys::map_anonymous(PAGE)?;
    let collision = sys::map_object_in(
        Handle::SELF_PROC,
        Handle::NONE,
        0,
        page,
        PAGE,
        prot::READ | prot::WRITE,
        map::FIXED,
    );
    let collision_errno = match collision {
        Ok(_) => {
            println!("CONTRACT K4 FAIL fixed-collision mapped");
            return Err(Errno::EIO);
        }
        Err(error) => error,
    };
    let unmapped_errno = match sys::unmap(Handle::SELF_PROC, page.saturating_add(PAGE), PAGE) {
        Ok(()) => {
            println!("CONTRACT K4 FAIL munmap-unmapped ok");
            return Err(Errno::EIO);
        }
        Err(error) => error,
    };
    let huge_errno = match sys::map_anonymous(1usize << 40) {
        Ok(_) => {
            println!("CONTRACT K4 FAIL huge-map ok");
            return Err(Errno::EIO);
        }
        Err(error) => error,
    };
    let protect_errno = match protect(Handle::SELF_PROC, page.saturating_add(PAGE), PAGE, 0) {
        Ok(()) => {
            println!("CONTRACT K4 FAIL mprotect-unmapped ok");
            return Err(Errno::EIO);
        }
        Err(error) => error,
    };
    let _ = sys::unmap(Handle::SELF_PROC, page, PAGE);

    // Access after munmap must fault in a child.
    let child = start_child_with(
        fault_write_entry as *const () as usize,
        child_stack_top(),
        page,
    )?;
    let code = wait_child(child)?;
    let _ = sys::unmap(Handle::SELF_PROC, page, PAGE);
    let ok = collision_errno == Errno::EEXIST
        && unmapped_errno == Errno::EINVAL
        && huge_errno == Errno::ENOMEM
        && protect_errno == Errno::ENOMEM
        && code == -11;
    if ok {
        println!(
            "CONTRACT K4 PASS fixed={} munmap={} huge={} prot={} after-unmap={code}",
            collision_errno.0, unmapped_errno.0, huge_errno.0, protect_errno.0
        );
        Ok(())
    } else {
        println!(
            "CONTRACT K4 FAIL fixed={} want=17 munmap={} want=22 huge={} want=12 prot={} want=12 after-unmap={code} want=-11",
            collision_errno.0, unmapped_errno.0, huge_errno.0, protect_errno.0
        );
        Err(Errno::EIO)
    }
}

fn contract_oom() -> Result<(), Errno> {
    let first = start_child_with(
        oom_child_entry as *const () as usize,
        child_stack_top(),
        OOM_OBJECT_BYTES,
    )?;
    let first_code = wait_child(first)?;
    let second = start_child_with(
        oom_child_entry as *const () as usize,
        child_stack_top(),
        OOM_OBJECT_BYTES,
    )?;
    let second_code = wait_child(second)?;
    let recovery = sys::map_anonymous(PAGE);
    let recovery_ok = match recovery {
        Ok(address) => {
            write_word(address, 0xA5A5_5A5A_1234_5678);
            let value = read_word(address);
            let _ = sys::unmap(Handle::SELF_PROC, address, PAGE);
            value == 0xA5A5_5A5A_1234_5678
        }
        Err(_) => false,
    };
    if first_code == 0 && second_code == 0 && recovery_ok {
        println!("CONTRACT K5 PASS first={first_code} second={second_code} recovery=1");
        Ok(())
    } else {
        println!(
            "CONTRACT K5 FAIL first={first_code} second={second_code} want=0 recovery={recovery_ok}"
        );
        Err(Errno::EIO)
    }
}

fn contract_stack() -> Result<(), Errno> {
    let sp = current_sp();
    let within = sp.saturating_sub(STACK_WITHIN);
    // SAFETY: the user stack VMA is 256 KiB and this access is 200 KiB below the current SP.
    let within_ok = unsafe {
        (within as *mut u8).write_volatile(0x5A);
        (within as *const u8).read_volatile() == 0x5A
    };
    if !within_ok {
        println!("CONTRACT K6 FAIL within-stack sp=0x{sp:x} address=0x{within:x}");
        return Err(Errno::EIO);
    }
    let child = start_child_with(stack_beyond_entry as *const () as usize, sp, sp)?;
    let code = wait_child(child)?;
    if code == -11 {
        println!("CONTRACT K6 PASS within=1 beyond-status={code}");
        Ok(())
    } else {
        println!("CONTRACT K6 FAIL beyond-status={code} want=-11");
        Err(Errno::EIO)
    }
}

fn contract_efault() -> Result<(), Errno> {
    let low = debug_write_raw(1, 1);
    let past = debug_write_raw(USER_END.saturating_add(1), 1);
    let wrap = debug_write_raw(USER_END.saturating_sub(1), 2);
    let uname = uname_raw(1);
    let clock = clock_get_raw(clock::MONOTONIC as usize, 1);

    // Direct PROT_NONE class: a valid user address whose leaf is present supervisor-only.
    let none = match sys::map_anonymous(PAGE) {
        Ok(address) => address,
        Err(error) => {
            println!("CONTRACT K7 FAIL protnone-map errno={}", error.0);
            return Err(error);
        }
    };
    if let Err(error) = protect(Handle::SELF_PROC, none, PAGE, 0) {
        println!("CONTRACT K7 FAIL protnone-protect errno={}", error.0);
        return Err(error);
    }
    let protnone_debug = debug_write_raw(none, 1);
    let (pipe_r, pipe_w) = match pipe_create() {
        Ok(pipe) => pipe,
        Err(error) => {
            println!("CONTRACT K7 FAIL pipe errno={}", error.0);
            return Err(error);
        }
    };
    // SAFETY: WRITE takes the pipe handle, the PROT_NONE user pointer and a length; the kernel
    // validates both and returns EFAULT before copying.
    let protnone_write = Errno::from_ret(unsafe {
        sys::syscall3(Syscall::WRITE as usize, pipe_w.0 as usize, none, 1)
    });
    let protnone_uname = uname_raw(none);
    let protnone_clock = clock_get_raw(clock::MONOTONIC as usize, none);
    let _ = sys::handle_close(pipe_r);
    let _ = sys::handle_close(pipe_w);
    let _ = sys::unmap(Handle::SELF_PROC, none, PAGE);

    let low_err = low.as_ref().err().copied();
    let past_err = past.as_ref().err().copied();
    let wrap_err = wrap.as_ref().err().copied();
    let uname_err = uname.as_ref().err().copied();
    let clock_err = clock.as_ref().err().copied();
    let protnone_debug_err = protnone_debug.as_ref().err().copied();
    let protnone_write_err = protnone_write.as_ref().err().copied();
    let protnone_uname_err = protnone_uname.as_ref().err().copied();
    let protnone_clock_err = protnone_clock.as_ref().err().copied();
    let low_ok = matches!(low_err, Some(Errno::EFAULT) | Some(Errno::EINVAL));
    let past_ok = matches!(past_err, Some(Errno::EFAULT) | Some(Errno::EINVAL));
    let wrap_ok = matches!(wrap_err, Some(Errno::EFAULT) | Some(Errno::EINVAL));
    let uname_ok = matches!(uname_err, Some(Errno::EFAULT) | Some(Errno::EINVAL));
    let clock_ok = matches!(clock_err, Some(Errno::EFAULT) | Some(Errno::EINVAL));
    let protnone_debug_ok = protnone_debug_err == Some(Errno::EFAULT);
    let protnone_write_ok = protnone_write_err == Some(Errno::EFAULT);
    let protnone_uname_ok = protnone_uname_err == Some(Errno::EFAULT);
    let protnone_clock_ok = protnone_clock_err == Some(Errno::EFAULT);
    if low_ok
        && past_ok
        && wrap_ok
        && uname_ok
        && clock_ok
        && protnone_debug_ok
        && protnone_write_ok
        && protnone_uname_ok
        && protnone_clock_ok
    {
        println!(
            "CONTRACT K7 PASS low={:?} past={:?} wrap={:?} uname={:?} clock={:?} pnone-debug={:?} pnone-write={:?} pnone-uname={:?} pnone-clock={:?}",
            low_err.map(|error| error.0),
            past_err.map(|error| error.0),
            wrap_err.map(|error| error.0),
            uname_err.map(|error| error.0),
            clock_err.map(|error| error.0),
            protnone_debug_err.map(|error| error.0),
            protnone_write_err.map(|error| error.0),
            protnone_uname_err.map(|error| error.0),
            protnone_clock_err.map(|error| error.0)
        );
        Ok(())
    } else {
        println!(
            "CONTRACT K7 FAIL low={:?} past={:?} wrap={:?} uname={:?} clock={:?} pnone-debug={:?} pnone-write={:?} pnone-uname={:?} pnone-clock={:?}",
            low_err.map(|error| error.0),
            past_err.map(|error| error.0),
            wrap_err.map(|error| error.0),
            uname_err.map(|error| error.0),
            clock_err.map(|error| error.0),
            protnone_debug_err.map(|error| error.0),
            protnone_write_err.map(|error| error.0),
            protnone_uname_err.map(|error| error.0),
            protnone_clock_err.map(|error| error.0)
        );
        Err(Errno::EIO)
    }
}

fn oom_pattern(chunk: usize, page: usize, seed: usize) -> u64 {
    0xA5A5_0000_0000_0000u64 ^ ((seed as u64) << 48) ^ ((chunk as u64) << 20) ^ (page as u64)
}

/// In-guest frame-exhaustion child: map 1 MiB chunks until the frame allocator refuses, verify
/// every page carries a distinct pattern, then release everything.
extern "C" fn oom_inject_child_entry(packed: usize) -> ! {
    let shared = packed & 0x0000_ffff_ffff_ffff;
    let seed = packed >> 56;
    // SAFETY: `shared` is the parent's shared page, inherited at the same address; the first two
    // u32 words are the ready/go barrier.
    let ready = unsafe { &*(shared as *const AtomicU32) };
    // SAFETY: same shared page; `go` is the second u32.
    let go = unsafe { &*((shared.saturating_add(4)) as *const AtomicU32) };
    // SAFETY: same shared page; `verified` is the third u32. Both children hold every frame
    // until both have checked their patterns, so neither frees frames into the other's
    // allocation loop.
    let verified = unsafe { &*((shared.saturating_add(8)) as *const AtomicU32) };

    let small = match sys::map_anonymous(OOM_SMALL_PAGES * PAGE) {
        Ok(address) => address,
        Err(error) => {
            println!("oom: fail small-map errno={}", error.0);
            sys::exit(3);
        }
    };
    OOM_CHUNK_ADDR[0].store(small, Ordering::Release);
    OOM_CHUNK_PAGES_USED[0].store(OOM_SMALL_PAGES, Ordering::Release);
    ready.fetch_add(1, Ordering::Release);
    let mut spins = 0u32;
    while go.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            println!("oom: fail barrier timeout");
            sys::exit(9);
        }
    }

    let mut chunks = 1usize;
    let mut refusals = 0u32;
    while chunks < OOM_MAX_CHUNKS && refusals < OOM_REFUSALS {
        match sys::map_anonymous(OOM_CHUNK_BYTES) {
            Ok(address) => {
                OOM_CHUNK_ADDR[chunks].store(address, Ordering::Release);
                OOM_CHUNK_PAGES_USED[chunks].store(OOM_CHUNK_PAGES, Ordering::Release);
                chunks = chunks.saturating_add(1);
                refusals = 0;
            }
            Err(error) if error == Errno::ENOMEM => {
                refusals = refusals.saturating_add(1);
            }
            Err(error) => {
                println!("oom: fail map errno={}", error.0);
                sys::exit(4);
            }
        }
    }
    if refusals == 0 {
        println!("oom: fail no-refusal chunks={chunks}");
        sys::exit(7);
    }

    let mut chunk = 0usize;
    while chunk < chunks {
        let base = OOM_CHUNK_ADDR[chunk].load(Ordering::Acquire);
        let pages = OOM_CHUNK_PAGES_USED[chunk].load(Ordering::Acquire);
        let mut page = 0usize;
        while page < pages {
            write_word(
                base.saturating_add(page.saturating_mul(PAGE)),
                oom_pattern(chunk, page, seed),
            );
            page = page.saturating_add(1);
        }
        chunk = chunk.saturating_add(1);
    }
    chunk = 0;
    while chunk < chunks {
        let base = OOM_CHUNK_ADDR[chunk].load(Ordering::Acquire);
        let pages = OOM_CHUNK_PAGES_USED[chunk].load(Ordering::Acquire);
        let mut page = 0usize;
        while page < pages {
            let address = base.saturating_add(page.saturating_mul(PAGE));
            let observed = read_word(address);
            if observed != oom_pattern(chunk, page, seed) {
                println!(
                    "oom: fail double-handout chunk={chunk} page={page} seen={observed:#x} want={:#x}",
                    oom_pattern(chunk, page, seed)
                );
                sys::exit(5);
            }
            page = page.saturating_add(1);
        }
        chunk = chunk.saturating_add(1);
    }

    let mut frames_back = 0usize;
    // Hold every frame until both children have verified their patterns. If one child freed
    // while the other was still allocating, the freed frames could satisfy the other's next
    // chunk and hide the exhaustion this row is measuring.
    verified.fetch_add(1, Ordering::AcqRel);
    let mut spins = 0u32;
    while verified.load(Ordering::Acquire) < 2 {
        core::hint::spin_loop();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            println!("oom: fail verified barrier timeout");
            sys::exit(10);
        }
    }
    chunk = 0;
    while chunk < chunks {
        let base = OOM_CHUNK_ADDR[chunk].load(Ordering::Acquire);
        let pages = OOM_CHUNK_PAGES_USED[chunk].load(Ordering::Acquire);
        if let Err(error) = sys::unmap(Handle::SELF_PROC, base, pages.saturating_mul(PAGE)) {
            println!("oom: fail unmap errno={}", error.0);
            sys::exit(8);
        }
        frames_back = frames_back.saturating_add(pages.saturating_mul(PAGE));
        chunk = chunk.saturating_add(1);
    }
    println!("oom: pass refusals={refusals} frames_back={frames_back}");
    sys::exit(0)
}

fn oom_round(
    shared: usize,
    round: usize,
    ready: &AtomicU32,
    go: &AtomicU32,
    verified: &AtomicU32,
) -> Result<(), Errno> {
    ready.store(0, Ordering::Release);
    go.store(0, Ordering::Release);
    verified.store(0, Ordering::Release);
    println!("oom: round={round} children=2");

    let first_seed = round.saturating_mul(2).saturating_sub(1);
    let second_seed = round.saturating_mul(2);
    let first = start_child_with(
        oom_inject_child_entry as *const () as usize,
        child_stack_top(),
        (first_seed << 56) | shared,
    )?;
    let second = start_child_with(
        oom_inject_child_entry as *const () as usize,
        child_stack_top(),
        (second_seed << 56) | shared,
    )?;

    let mut spins = 0u32;
    while ready.load(Ordering::Acquire) < 2 {
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            println!("oom: fail children not ready");
            return Err(Errno::ETIME);
        }
    }
    go.store(1, Ordering::Release);
    let first_code = wait_child(first)?;
    let second_code = wait_child(second)?;
    if first_code != 0 || second_code != 0 {
        println!("oom: fail child codes first={first_code} second={second_code}");
        return Err(Errno::EIO);
    }
    // Drain the children's `s24: frames free=` lines before the round marker; the dying CPU
    // prints them on its own context and `PROC_WAIT` can wake the parent first.
    let _ = sys::nanosleep(200_000_000);
    println!("oom: round={round} done");

    let next = sys::map_anonymous(PAGE)?;
    write_word(next, 0x4F4F_4D5F_4F4B_0001);
    if read_word(next) != 0x4F4F_4D5F_4F4B_0001 {
        let _ = sys::unmap(Handle::SELF_PROC, next, PAGE);
        println!("oom: fail parent pattern");
        return Err(Errno::EIO);
    }
    let _ = sys::unmap(Handle::SELF_PROC, next, PAGE);
    println!("oom: parent-ok next={PAGE}");
    Ok(())
}

fn oom_injection() -> Result<(), Errno> {
    let object = memobj_create_raw(PAGE)?;
    let shared = sys::map_object_in(
        Handle::SELF_PROC,
        object,
        0,
        0,
        PAGE,
        prot::READ | prot::WRITE,
        map::SHARED,
    )?;
    // SAFETY: the shared page is mapped writable; the first two u32 words are the barrier.
    let ready = unsafe { &*(shared as *const AtomicU32) };
    // SAFETY: same shared page; `go` is the second u32.
    let go = unsafe { &*((shared.saturating_add(4)) as *const AtomicU32) };
    // SAFETY: same shared page; `verified` is the third u32.
    let verified = unsafe { &*((shared.saturating_add(8)) as *const AtomicU32) };

    let mut result = Ok(());
    for round in 1..=2usize {
        if let Err(error) = oom_round(shared, round, ready, go, verified) {
            result = Err(error);
            break;
        }
    }
    let _ = sys::unmap(Handle::SELF_PROC, shared, PAGE);
    let _ = sys::handle_close(object);
    result
}

fn contract_suite() -> Result<(), Errno> {
    let mut failed = 0usize;
    if contract_fault_unmapped().is_err() {
        failed = failed.saturating_add(1);
    }
    if contract_protection().is_err() {
        failed = failed.saturating_add(1);
    }
    if contract_cow().is_err() {
        failed = failed.saturating_add(1);
    }
    if contract_mmap_errno().is_err() {
        failed = failed.saturating_add(1);
    }
    if contract_oom().is_err() {
        failed = failed.saturating_add(1);
    }
    if contract_stack().is_err() {
        failed = failed.saturating_add(1);
    }
    if contract_efault().is_err() {
        failed = failed.saturating_add(1);
    }
    if failed == 0 {
        println!("smp-memory: contract PASS 7/7");
        Ok(())
    } else {
        println!("smp-memory: contract FAIL {failed}/7");
        Err(Errno::EIO)
    }
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

    // Four-way concurrent unmap barrier: three workers and the controller. The three workers
    // have parked distinct 16-page mappings; the controller maps its own before releasing them.
    if let Err(error) = wait_worker_flags(&WAITER_PREP) {
        println!("smp-memory: shootdown waiters not ready errno={}", error.0);
        return 1;
    }
    let barrier_len = MAP_PAGES.saturating_mul(PAGE);
    let controller_map = match sys::map_anonymous(barrier_len) {
        Ok(base) => base,
        Err(error) => {
            println!("smp-memory: controller waiter map errno={}", error.0);
            return 1;
        }
    };
    println!(
        "smp-memory: concurrent-shootdown waiters={}",
        WORKERS.saturating_add(1)
    );
    WAITER_GO.store(1, Ordering::Release);
    if let Err(error) = sys::unmap(Handle::SELF_PROC, controller_map, barrier_len) {
        println!("smp-memory: controller waiter unmap errno={}", error.0);
        return 1;
    }
    if let Err(error) = wait_worker_flags(&WAITER_DONE) {
        println!(
            "smp-memory: shootdown waiters did not finish errno={}",
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

    if let Err(error) = contract_suite() {
        println!("smp-memory: contract suite failed errno={}", error.0);
        fail("contract");
        return 1;
    }

    if let Err(error) = oom_injection() {
        println!("oom: failed errno={}", error.0);
        fail("oom");
        return 1;
    }

    let _ = sys::unmap(Handle::SELF_PROC, sampled, PAGE);
    let _ = sys::unmap(Handle::SELF_PROC, keep, 2usize.saturating_mul(PAGE));
    let _ = sys::handle_close(object);
    // Let the child-exit console lines drain before the one-line summary; the console is a
    // shared byte stream and the qtest matches the summary as a whole line.
    let _ = sys::nanosleep(100_000_000);

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
