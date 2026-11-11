// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S35 fixture: handle generation retirement, rights extraction and close ordering.
//!
//! It runs as `/init` so it owns the well-known handles 0..=3. The three sections print one
//! `s35:` line each; a mismatch prints the failing cycle or errno and exits non-zero.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{syscall1, syscall2, syscall6};

entry!(main);

/// Terminate the fixture deterministically when Rust reports a panic.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    raven_rt::exit(2)
}

/// Handles created and closed by the generation test.
const CYCLES: usize = 300;
/// The close that exhausts generation 255 and retires the first slot.
const RETIRE_AFTER: usize = 256;
/// Scratch user addresses for the rights mappings.
const MAP_BASE: usize = 0x3000_0000;
const PAGE: usize = 4096;
const PATTERN: u8 = 0x5a;
const ADD: usize = 1;
const SIGNALED: usize = 8;
const COOKIE: u64 = 0x5353_0035;

/// `EvqEvent` from the S26 ABI.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EvqEvent {
    cookie: u64,
    events: u32,
    pad: u32,
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    u32::try_from(result(raw)?)
        .map(Handle)
        .map_err(|_| Errno::EINVAL)
}

fn memobj_create() -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes a page length and zero flags; the kernel validates both.
    handle(unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, PAGE, 0) })
}

fn close(handle: Handle) -> Result<(), Errno> {
    // SAFETY: HANDLE_CLOSE takes one scalar handle validated by the kernel.
    result(unsafe { syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) }).map(|_| ())
}

fn dup(object: Handle, rights: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_DUP follows the stable syscall ABI; the kernel validates the handle.
    handle(unsafe {
        syscall2(
            Syscall::HANDLE_DUP as usize,
            object.0 as usize,
            rights.0 as usize,
        )
    })
}

fn transfer(process: Handle, object: Handle, rights: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_TRANSFER follows the stable syscall ABI; the kernel validates every handle.
    handle(unsafe {
        syscall6(
            Syscall::HANDLE_TRANSFER as usize,
            process.0 as usize,
            object.0 as usize,
            rights.0 as usize,
            0,
            0,
            0,
        )
    })
}

fn map_at(object: Handle, address: usize, flags: usize) -> Result<usize, Errno> {
    // SAFETY: MAP follows the stable syscall ABI; the kernel validates the object, address and
    // protection.
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            address,
            PAGE,
            flags,
        )
    })
}

fn evq_create() -> Result<Handle, Errno> {
    // SAFETY: EVQ_CREATE takes a flags word; zero is the only accepted value.
    handle(unsafe { syscall1(Syscall::EVQ_CREATE as usize, 0) })
}

fn event_create() -> Result<Handle, Errno> {
    // SAFETY: EVENT_CREATE takes a flags word; zero is the only accepted value.
    handle(unsafe { syscall1(Syscall::EVENT_CREATE as usize, 0) })
}

fn evq_ctl(
    queue: Handle,
    operation: usize,
    source: Handle,
    events: usize,
    cookie: u64,
) -> Result<(), Errno> {
    // SAFETY: EVQ_CTL follows the stable syscall ABI; the kernel validates the queue and source.
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

fn evq_wait(queue: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: `out` is writable for `out.len()` records and the kernel bounds the copy.
    result(unsafe {
        syscall6(
            Syscall::EVQ_WAIT as usize,
            queue.0 as usize,
            out.as_mut_ptr() as usize,
            out.len(),
            timeout_ns as usize,
            0,
            0,
        )
    })
}

fn event_signal(event: Handle) -> Result<(), Errno> {
    // SAFETY: EVENT_SIGNAL takes a scalar handle and value; the kernel validates the handle.
    result(unsafe { syscall2(Syscall::EVENT_SIGNAL as usize, event.0 as usize, 0) }).map(|_| ())
}

/// 300 create/close cycles on the first free slot. The 256th close retires it, so the rest of the
/// cycles land on the next slot and the cycle-1 handle stays stale forever.
fn stale_handles() -> Result<(), Errno> {
    let first = memobj_create()?;
    if first.generation() != 0 || first.index() < 4 {
        println!("s35: first handle {:#x}", first.0);
        return Err(Errno::EIO);
    }
    let slot = first.index();
    let mut current = first;
    for cycle in 1..CYCLES {
        close(current)?;
        let next = memobj_create()?;
        let (expected_slot, expected_generation) = if cycle < RETIRE_AFTER {
            (slot, cycle as u8)
        } else {
            (slot + 1, (cycle - RETIRE_AFTER) as u8)
        };
        if next.index() != expected_slot || next.generation() != expected_generation {
            println!(
                "s35: cycle {} handle {:#x} want slot {} generation {}",
                cycle, next.0, expected_slot, expected_generation
            );
            return Err(Errno::EIO);
        }
        current = next;
    }
    // The closing count is now 300: the final live handle closes here.
    close(current)?;
    if close(first) != Err(Errno::EBADF) {
        println!("s35: cycle-1 handle is not stale");
        return Err(Errno::EIO);
    }
    if dup(first, Rights::READ) != Err(Errno::EBADF) {
        println!("s35: stale dup did not fail");
        return Err(Errno::EIO);
    }
    let fresh = memobj_create()?;
    if fresh.index() != slot + 1 || fresh.generation() != 44 {
        println!("s35: fresh handle {:#x}", fresh.0);
        return Err(Errno::EIO);
    }
    close(fresh)?;
    Ok(())
}

/// Transfer a memory object to the caller's own process and probe every memory-applicable right
/// on the installed copy: MAP+WRITE, DUP, TRANSFER, and the narrowing intersection.
fn rights() -> Result<(), Errno> {
    let memory = memobj_create()?;
    let transferred = transfer(Handle::SELF_PROC, memory, Rights::ALL)?;
    let mapped = map_at(
        transferred,
        MAP_BASE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    // SAFETY: MAP just returned a writable one-page shared mapping at MAP_BASE.
    unsafe { (mapped as *mut u8).write_volatile(PATTERN) };

    // READ-only duplicate: it keeps READ but loses MAP, so mapping it is refused.
    let read_only = dup(transferred, Rights::READ)?;
    if map_at(
        read_only,
        MAP_BASE + PAGE,
        map::pack(prot::READ, map::FIXED),
    ) != Err(Errno::EACCES)
    {
        println!("s35: READ duplicate kept MAP");
        return Err(Errno::EIO);
    }

    // Re-transfer the duplicate with READ|MAP and prove the narrow mask arrived and the shared
    // writable mapping is refused.
    let dup_all = dup(transferred, Rights::ALL)?;
    let narrowed = transfer(Handle::SELF_PROC, dup_all, Rights::READ.union(Rights::MAP))?;
    let narrowed_map = map_at(
        narrowed,
        MAP_BASE + 2 * PAGE,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: MAP just returned a readable one-page mapping of the pattern page.
    if unsafe { (narrowed_map as *const u8).read_volatile() } != PATTERN {
        println!("s35: narrowed mapping has the wrong pattern");
        return Err(Errno::EIO);
    }
    if map_at(
        narrowed,
        MAP_BASE + 3 * PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    ) != Err(Errno::EACCES)
    {
        println!("s35: narrowed handle kept WRITE");
        return Err(Errno::EIO);
    }

    close(read_only)?;
    close(dup_all)?;
    close(narrowed)?;
    close(transferred)?;
    close(memory)?;
    Ok(())
}

/// A bad close must leave the registration live; a good close must remove it. The observer
/// duplicate keeps the user event object alive after the registered handle is closed.
fn close_order() -> Result<(), Errno> {
    let queue = evq_create()?;
    let event = event_create()?;
    let observer = dup(
        event,
        Rights::READ
            .union(Rights::SIGNAL)
            .union(Rights::DUP)
            .union(Rights::TRANSFER),
    )?;
    evq_ctl(queue, ADD, event, SIGNALED, COOKIE)?;

    let bad = Handle::new(event.index(), event.generation().wrapping_add(1));
    if close(bad) != Err(Errno::EBADF) {
        println!("s35: bad close was accepted");
        return Err(Errno::EIO);
    }
    event_signal(event)?;
    let mut events = [EvqEvent::default(); 4];
    let count = evq_wait(queue, &mut events, 10_000_000)?;
    if count != 1 || events.first().map(|event| event.cookie) != Some(COOKIE) {
        println!("s35: bad close deregistered the event (count {count})");
        return Err(Errno::EIO);
    }

    close(event)?;
    event_signal(observer)?;
    let count = evq_wait(queue, &mut events, 20_000_000)?;
    if count != 0 {
        println!("s35: good close left the event registered (count {count})");
        return Err(Errno::EIO);
    }

    close(observer)?;
    close(queue)?;
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(error) = stale_handles() {
        println!("s35: stale handles failed {}", error.0);
        return 10;
    }
    println!("s35: stale handles ok");
    if let Err(error) = rights() {
        println!("s35: rights failed {}", error.0);
        return 11;
    }
    println!("s35: rights ok");
    if let Err(error) = close_order() {
        println!("s35: close order failed {}", error.0);
        return 12;
    }
    println!("s35: close order ok");
    0
}
