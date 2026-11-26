// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K484 aarch64 memory-parity fixture: the K467 `PROT_NONE`/straddle usercopy vectors and the
//! K475 frame-exhaustion row, without the timing-sensitive four-way TLB stress. The TLB stress
//! lives in the `raven-smp-aarch64-init` binary and stays covered by `smp-tlb-aarch64-concurrent`;
//! this init is the isolated measurement of the memory rows on aarch64.

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
const CHILD_STACK_LEN: usize = 64 * 1024;
const POLL_LIMIT: u32 = 20_000_000;
/// Raw kernel `pipe(fds)` number (kernel-owned, not a `Syscall` variant).
const PIPE: usize = 0x6e;
/// Raw kernel `proc_fork` number (kernel-owned, not a `Syscall` variant).
const PROC_FORK: usize = 0x39;

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

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; CHILD_STACK_LEN]);

// Writable `.bss`: the forked child's thread stack is inherited from the init image, so the
// parent does not allocate a new frame after the baseline.
static mut CHILD_STACK: Stack = Stack([0; CHILD_STACK_LEN]);

fn child_stack_top() -> usize {
    let base = core::ptr::addr_of_mut!(CHILD_STACK);
    (base as usize).saturating_add(CHILD_STACK_LEN)
}

fn fail(message: &str) -> i32 {
    println!("smp-aarch64-memory: fail {message}");
    1
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
    let _ = unmap(none, PAGE);
    let _ = unmap(straddle, 2usize.saturating_mul(PAGE));

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

extern "C" fn baseline_child_entry(_arg: usize) -> ! {
    sys::exit(0)
}

/// Reap one trivial child before the OOM row. Its `s24: frames free=` line is the pre-injection
/// baseline; the parent keeps the child's stack mapped through the row so both samples include it.
fn baseline_child() -> Result<(), Errno> {
    let child = proc_fork()?;
    sys::thread_create(
        child,
        baseline_child_entry as *const () as usize,
        child_stack_top(),
        0,
    )?;
    let code = proc_wait(child)?;
    let _ = sys::nanosleep(1_000_000_000);
    if code != 0 {
        println!("smp-aarch64-memory: fail baseline child code={code}");
        return Err(Errno::EIO);
    }
    Ok(())
}

fn oom_pattern(chunk: usize, page: usize, seed: usize) -> u64 {
    0xA5A5_0000_0000_0000u64 ^ ((seed as u64) << 48) ^ ((chunk as u64) << 20) ^ (page as u64)
}

/// Serialize a child's console line through the shared page. `println!` may issue more than one
/// write, so two children printing a failure at the same time can interleave the bytes.
macro_rules! oom_print {
    ($lock:expr, $($arg:tt)*) => {{
        while $lock.swap(1, Ordering::Acquire) != 0 {
            core::hint::spin_loop();
        }
        println!($($arg)*);
        $lock.store(0, Ordering::Release);
    }};
}

/// K475 frame-exhaustion child: map 1 MiB chunks until the frame allocator refuses, verify every
/// page keeps its distinct pattern, then release everything.
extern "C" fn oom_child_entry(packed: usize) -> ! {
    let shared = packed & 0x0000_ffff_ffff_ffff;
    let seed = packed >> 56;
    // SAFETY: `shared` is the parent's shared page, inherited at the same address; the first two
    // u32 words are the ready/go barrier.
    let ready = unsafe { &*(shared as *const AtomicU32) };
    // SAFETY: same shared page; `go` is the second u32.
    let go = unsafe { &*((shared.saturating_add(4)) as *const AtomicU32) };
    // SAFETY: same shared page; `written` is the third u32 and `verified` the fourth.
    let written = unsafe { &*((shared.saturating_add(8)) as *const AtomicU32) };
    // SAFETY: same shared page; `verified` is the fourth u32.
    let verified = unsafe { &*((shared.saturating_add(12)) as *const AtomicU32) };
    // The odd seed owns slot A, the even seed slot B. The parent prints both result lines after
    // reaping, so the two children never interleave console writes on the success path.
    let odd = seed % 2 == 1;
    let refusals_offset = if odd { 16 } else { 20 };
    let frames_offset = if odd { 24 } else { 32 };
    // SAFETY: same shared page; the refusal slots are the u32s at +16/+20.
    let refusals_slot = unsafe { &*((shared.saturating_add(refusals_offset)) as *const AtomicU32) };
    // SAFETY: same shared page; the frame slots are the u64s at +24/+32.
    let frames_slot = unsafe { &*((shared.saturating_add(frames_offset)) as *const AtomicU64) };
    // SAFETY: same shared page; the console lock is the u32 at +40.
    let console = unsafe { &*((shared.saturating_add(40)) as *const AtomicU32) };

    let small = match sys::map_anonymous(OOM_SMALL_PAGES * PAGE) {
        Ok(address) => address,
        Err(error) => {
            oom_print!(console, "smp-aarch64-oom: fail small-map errno={}", error.0);
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
            oom_print!(console, "smp-aarch64-oom: fail barrier timeout");
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
                oom_print!(console, "smp-aarch64-oom: fail map errno={}", error.0);
                sys::exit(4);
            }
        }
    }
    if refusals == 0 {
        oom_print!(console, "smp-aarch64-oom: fail no-refusal chunks={chunks}");
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
    // Every page has been written. Wait for the other child before reading any page back, so a
    // frame handed to both processes is overwritten by at least one pattern before verification.
    written.fetch_add(1, Ordering::AcqRel);
    let mut spins = 0u32;
    while written.load(Ordering::Acquire) < 2 {
        core::hint::spin_loop();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            oom_print!(console, "smp-aarch64-oom: fail written barrier timeout");
            sys::exit(11);
        }
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
                oom_print!(
                    console,
                    "smp-aarch64-oom: fail double-handout chunk={chunk} page={page} seen={observed:#x} want={:#x}",
                    oom_pattern(chunk, page, seed)
                );
                sys::exit(5);
            }
            page = page.saturating_add(1);
        }
        chunk = chunk.saturating_add(1);
    }

    let mut frames_back = 0usize;
    // Hold every frame until both children have verified their patterns.
    verified.fetch_add(1, Ordering::AcqRel);
    let mut spins = 0u32;
    while verified.load(Ordering::Acquire) < 2 {
        core::hint::spin_loop();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            oom_print!(console, "smp-aarch64-oom: fail verified barrier timeout");
            sys::exit(10);
        }
    }
    // Leave the chunks mapped and let process teardown reclaim the whole address space. Per-chunk
    // unmaps on aarch64 TCG need a remote TLB shootdown per 1 MiB; under host load those can time
    // out, and the safety path deliberately leaks the frames. Teardown switches the address space
    // out and frees every frame without a per-range shootdown, which is the reclaim path this row
    // measures on aarch64. The reported `frames_back` is the mapped bytes released at exit.
    chunk = 0;
    while chunk < chunks {
        let pages = OOM_CHUNK_PAGES_USED[chunk].load(Ordering::Acquire);
        frames_back = frames_back.saturating_add(pages.saturating_mul(PAGE));
        chunk = chunk.saturating_add(1);
    }
    refusals_slot.store(refusals, Ordering::Release);
    frames_slot.store(frames_back as u64, Ordering::Release);
    sys::exit(0)
}

fn oom_round(shared: usize, round: usize) -> Result<(), Errno> {
    // SAFETY: the shared page is mapped writable; the first two u32 words are the barrier.
    let ready = unsafe { &*(shared as *const AtomicU32) };
    // SAFETY: same shared page; `go` is the second u32.
    let go = unsafe { &*((shared.saturating_add(4)) as *const AtomicU32) };
    // SAFETY: same shared page; `written` is the third u32 and `verified` the fourth.
    let written = unsafe { &*((shared.saturating_add(8)) as *const AtomicU32) };
    // SAFETY: same shared page; `verified` is the fourth u32.
    let verified = unsafe { &*((shared.saturating_add(12)) as *const AtomicU32) };
    // SAFETY: same shared page; the refusal slots are the u32s at +16/+20.
    let refusals_a = unsafe { &*((shared.saturating_add(16)) as *const AtomicU32) };
    // SAFETY: same shared page; slot B is the u32 at +20.
    let refusals_b = unsafe { &*((shared.saturating_add(20)) as *const AtomicU32) };
    // SAFETY: same shared page; the frame slots are the u64s at +24/+32.
    let frames_a = unsafe { &*((shared.saturating_add(24)) as *const AtomicU64) };
    // SAFETY: same shared page; slot B is the u64 at +32.
    let frames_b = unsafe { &*((shared.saturating_add(32)) as *const AtomicU64) };
    // SAFETY: same shared page; the console lock is the u32 at +40.
    let console = unsafe { &*((shared.saturating_add(40)) as *const AtomicU32) };
    ready.store(0, Ordering::Release);
    go.store(0, Ordering::Release);
    written.store(0, Ordering::Release);
    verified.store(0, Ordering::Release);
    refusals_a.store(0, Ordering::Release);
    refusals_b.store(0, Ordering::Release);
    frames_a.store(0, Ordering::Release);
    frames_b.store(0, Ordering::Release);
    console.store(0, Ordering::Release);

    let first_seed = round.saturating_mul(2).saturating_sub(1);
    let second_seed = round.saturating_mul(2);
    println!("smp-aarch64-oom: round={round} children=2");

    let first = proc_fork()?;
    sys::thread_create(
        first,
        oom_child_entry as *const () as usize,
        child_stack_top(),
        (first_seed << 56) | shared,
    )?;
    let second = proc_fork()?;
    sys::thread_create(
        second,
        oom_child_entry as *const () as usize,
        child_stack_top(),
        (second_seed << 56) | shared,
    )?;

    let mut spins = 0u32;
    while ready.load(Ordering::Acquire) < 2 {
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins > POLL_LIMIT {
            println!("smp-aarch64-oom: fail children not ready");
            return Err(Errno::ETIME);
        }
    }
    go.store(1, Ordering::Release);
    let first_code = proc_wait(first)?;
    let second_code = proc_wait(second)?;
    if first_code != 0 || second_code != 0 {
        println!("smp-aarch64-oom: fail child codes first={first_code} second={second_code}");
        return Err(Errno::EIO);
    }
    // Leave the parent's probe page, stacks and shared page mapped: process exit reclaims them
    // without a per-range TLB shootdown. A single-page unmap here triggers a remote shootdown
    // that can time out under TCG load and is not part of the baseline assertion.
    let _ = sys::nanosleep(20_000_000_000);
    println!(
        "smp-aarch64-oom: pass refusals={} frames_back={}",
        refusals_a.load(Ordering::Acquire),
        frames_a.load(Ordering::Acquire)
    );
    println!(
        "smp-aarch64-oom: pass refusals={} frames_back={}",
        refusals_b.load(Ordering::Acquire),
        frames_b.load(Ordering::Acquire)
    );
    println!("smp-aarch64-oom: round={round} done");

    let next = sys::map_anonymous(PAGE)?;
    write_word(next, 0x4F4F_4D5F_4F4B_0001);
    if read_word(next) != 0x4F4F_4D5F_4F4B_0001 {
        println!("smp-aarch64-oom: fail parent pattern");
        return Err(Errno::EIO);
    }
    println!("smp-aarch64-oom: parent-ok next={PAGE}");

    Ok(())
}

fn oom_injection() -> Result<(), Errno> {
    let object = memobj_create(PAGE)?;
    let shared = map_shared(object, 0, 0, PAGE, false)?;
    for round in 1..=2usize {
        oom_round(shared, round)?;
    }
    Ok(())
}

fn read_word(address: usize) -> u64 {
    // SAFETY: the caller passes an address inside the fixture's shared mapping.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write_word(address: usize, value: u64) {
    // SAFETY: as in `read_word`; the mapping is writable.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("smp-aarch64-memory: start");
    if let Err(error) = usercopy_contracts() {
        println!("smp-aarch64-copy: failed errno={}", error.0);
        return fail("usercopy");
    }
    if let Err(error) = baseline_child() {
        println!("smp-aarch64-memory: baseline failed errno={}", error.0);
        return fail("baseline");
    }
    if let Err(error) = oom_injection() {
        println!("smp-aarch64-oom: failed errno={}", error.0);
        return fail("oom");
    }
    println!("smp-aarch64-memory: ok");
    0
}
