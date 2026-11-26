// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K528 capability-lineage race fixture.
//!
//! Two user threads race `HANDLE_TRANSFER`/`HANDLE_DUP` (T1) against `HANDLE_REVOKE` (T2) on a
//! shared parent lineage. T1 installs children before T2's revoke completes; the kernel
//! registers each child after the install. The fixture then uses every installed child with
//! `MAP`: a completed ancestor revoke must make each use fail (`EPERM` when the child held the
//! right, `EACCES` when it never had it), never succeed.
//!
//! The suite runs three shapes: a direct parent, a closed intermediate (parent > intermediate >
//! leaf, with the intermediate handle closed), and a queued `SCM_RIGHTS` transfer. The
//! deterministic interleaving is the host test in `crates/raven-handles/src/lib_ut.rs`; this
//! guest proves the same invariant on 1 and 4 CPUs under real scheduling.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscall arguments and thread stacks; each block has its SAFETY note.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, clock, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

/// Page size for the memory objects used as capability carriers.
const PAGE: usize = 4096;
/// Thread stack size.
const STACK_SIZE: usize = 32 * 1024;
/// Transfers or dups in one race burst.
const TRANSFERS: usize = 4096;
/// Maximum stored children.
const MAX_CHILDREN: usize = TRANSFERS;
/// Deadline for a worker or a helper wait.
const WAIT_NS: u64 = 10_000_000_000;
/// Delay between the first completed operation and the revoke (a scheduling window).
const REVOKE_DELAY_NS: u64 = 100_000;

/// Raw duplex numbers used by the queued `SCM_RIGHTS` shape.
const DUPLEX_PAIR: usize = 0x110;
/// Raw duplex sendmsg.
const DUPLEX_SENDMSG: usize = 0x112;
/// Raw duplex recvmsg.
const DUPLEX_RECVMSG: usize = 0x113;
/// `MSG_NOSIGNAL`.
const DUPLEX_MSG_NOSIGNAL: u32 = 0x4000;
/// Control header bytes.
const CMSG_BYTES: usize = 16;

/// Parent/source handle for the current worker race.
static PARENT: AtomicU32 = AtomicU32::new(u32::MAX);
/// Ancestor handle T2 revokes.
static REVOKE_TARGET: AtomicU32 = AtomicU32::new(u32::MAX);
/// Worker shape: 0 = transfer, 1 = dup.
static WORKER_MODE: AtomicU32 = AtomicU32::new(0);
/// T1 reached its gate.
static T1_READY: AtomicU32 = AtomicU32::new(0);
/// T2 reached its gate.
static T2_READY: AtomicU32 = AtomicU32::new(0);
/// T1 completed its first operation.
static FIRST_OP: AtomicU32 = AtomicU32::new(0);
/// T2 completed the revoke.
static REVOKE_DONE: AtomicU32 = AtomicU32::new(0);
/// T2's revoke succeeded.
static REVOKE_OK: AtomicU32 = AtomicU32::new(0);
/// Stored children.
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// Installed child handles; `u32::MAX` is an empty slot.
static CHILDREN: [AtomicU32; MAX_CHILDREN] = [const { AtomicU32::new(u32::MAX) }; MAX_CHILDREN];

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// Writable `.bss`: an immutable static would land in `.rodata` and `THREAD_CREATE` rejects it.
static mut STACKS: [Stack; 2] = [const { Stack([0; STACK_SIZE]) }; 2];

#[repr(C)]
struct Message {
    data: u64,
    data_len: u64,
    control: u64,
    control_len: u64,
    flags: u32,
    handle_count: u32,
}

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the memory is used as a kernel-provided thread stack.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < 2` and the array lives for the whole fixture.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE)
}

fn now_ns() -> u64 {
    match sys::clock_get(clock::MONOTONIC) {
        Ok(time) => u64::try_from(time.tv_sec)
            .unwrap_or(0)
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::try_from(time.tv_nsec).unwrap_or(0)),
        Err(_) => 0,
    }
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn memobj_create() -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes a page-aligned length and zero flags.
    handle(unsafe { sys::syscall2(Syscall::MEMOBJ_CREATE as usize, PAGE, 0) })
}

fn dup(source: Handle, mask: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_DUP takes a live handle and a rights mask.
    handle(unsafe {
        sys::syscall2(
            Syscall::HANDLE_DUP as usize,
            source.0 as usize,
            mask.0 as usize,
        )
    })
}

fn transfer(destination: Handle, source: Handle, mask: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_TRANSFER takes destination and source handles and a rights mask.
    handle(unsafe {
        sys::syscall3(
            Syscall::HANDLE_TRANSFER as usize,
            destination.0 as usize,
            source.0 as usize,
            mask.0 as usize,
        )
    })
}

fn revoke(handle: Handle, mask: Rights) -> Result<(), Errno> {
    // SAFETY: HANDLE_REVOKE takes a live handle and a rights mask.
    result(unsafe {
        sys::syscall2(
            Syscall::HANDLE_REVOKE as usize,
            handle.0 as usize,
            mask.0 as usize,
        )
    })
    .map(|_| ())
}

fn close(handle: Handle) {
    // SAFETY: HANDLE_CLOSE takes one handle value.
    let _ = result(unsafe { sys::syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) });
}

fn map_read(object: Handle) -> Result<usize, Errno> {
    sys::map_object(object, 0, PAGE, prot::READ, false)
}

fn wait_for(flag: &AtomicU32) -> bool {
    let start = now_ns();
    while flag.load(Ordering::Acquire) == 0 {
        if now_ns().saturating_sub(start) > WAIT_NS {
            return false;
        }
        sys::yield_now();
    }
    true
}

fn put_u32(buffer: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = buffer.get_mut(offset..offset.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn get_u32(buffer: &[u8], offset: usize) -> Option<u32> {
    let slot = buffer.get(offset..offset.saturating_add(4))?;
    let bytes: [u8; 4] = slot.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn control_one(handle: Handle, buffer: &mut [u8]) -> usize {
    let length = CMSG_BYTES.saturating_add(4);
    put_u32(buffer, 0, length as u32);
    put_u32(buffer, 4, 1); // SOL_SOCKET
    put_u32(buffer, 8, 1); // SCM_RIGHTS
    put_u32(buffer, 12, 1);
    put_u32(buffer, CMSG_BYTES, handle.0);
    length
}

fn duplex_pair() -> Result<(Handle, Handle), Errno> {
    let mut ends = [0u32; 2];
    // SAFETY: DUPLEX_PAIR writes two handles to the supplied array.
    result(unsafe { sys::syscall2(DUPLEX_PAIR, ends.as_mut_ptr() as usize, 0) })?;
    Ok((Handle(ends[0]), Handle(ends[1])))
}

fn sendmsg(sender: Handle, data: &[u8], control: &[u8]) -> Result<usize, Errno> {
    let mut message = Message {
        data: data.as_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_ptr() as u64,
        control_len: control.len() as u64,
        flags: DUPLEX_MSG_NOSIGNAL,
        handle_count: 0,
    };
    // SAFETY: DUPLEX_SENDMSG reads the message header and the two buffers.
    result(unsafe {
        sys::syscall2(
            DUPLEX_SENDMSG,
            sender.0 as usize,
            (&mut message as *mut Message) as usize,
        )
    })
}

fn recvmsg(
    receiver: Handle,
    data: &mut [u8],
    control: &mut [u8],
) -> Result<(usize, Message), Errno> {
    let mut message = Message {
        data: data.as_mut_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_mut_ptr() as u64,
        control_len: control.len() as u64,
        flags: 0,
        handle_count: 0,
    };
    // SAFETY: DUPLEX_RECVMSG fills the message header and writes into the two buffers.
    let copied = result(unsafe {
        sys::syscall2(
            DUPLEX_RECVMSG,
            receiver.0 as usize,
            (&mut message as *mut Message) as usize,
        )
    })?;
    Ok((copied, message))
}

fn reset_race() {
    PARENT.store(u32::MAX, Ordering::Release);
    REVOKE_TARGET.store(u32::MAX, Ordering::Release);
    T1_READY.store(0, Ordering::Release);
    T2_READY.store(0, Ordering::Release);
    FIRST_OP.store(0, Ordering::Release);
    REVOKE_DONE.store(0, Ordering::Release);
    REVOKE_OK.store(0, Ordering::Release);
    COUNT.store(0, Ordering::Release);
    for slot in CHILDREN.iter() {
        slot.store(u32::MAX, Ordering::Release);
    }
}

fn record_child(child: Handle) {
    let index = COUNT.fetch_add(1, Ordering::AcqRel);
    if let Some(slot) = CHILDREN.get(index) {
        slot.store(child.0, Ordering::Release);
    }
}

extern "C" fn derive_worker(_argument: usize) -> ! {
    T1_READY.store(1, Ordering::Release);
    if !wait_for(&T2_READY) {
        sys::thread_exit(1);
    }
    let source = Handle(PARENT.load(Ordering::Acquire));
    let mask = Rights::MAP.union(Rights::READ);
    let mut index = 0usize;
    while index < TRANSFERS {
        let child = if WORKER_MODE.load(Ordering::Acquire) == 1 {
            dup(source, mask.union(Rights::TRANSFER))
        } else {
            transfer(Handle::SELF_PROC, source, mask)
        };
        if let Ok(child) = child {
            record_child(child);
            if index == 0 {
                FIRST_OP.store(1, Ordering::Release);
            }
        }
        index = index.saturating_add(1);
    }
    sys::thread_exit(0)
}

extern "C" fn revoke_worker(_argument: usize) -> ! {
    T2_READY.store(1, Ordering::Release);
    if !wait_for(&FIRST_OP) {
        REVOKE_DONE.store(1, Ordering::Release);
        sys::thread_exit(1);
    }
    let _ = sys::nanosleep(REVOKE_DELAY_NS);
    let target = Handle(REVOKE_TARGET.load(Ordering::Acquire));
    if revoke(target, Rights::MAP).is_ok() {
        REVOKE_OK.store(1, Ordering::Release);
    }
    REVOKE_DONE.store(1, Ordering::Release);
    sys::thread_exit(0)
}

fn start_race(mode: u32, source: Handle, target: Handle) -> Result<(), Errno> {
    reset_race();
    WORKER_MODE.store(mode, Ordering::Release);
    PARENT.store(source.0, Ordering::Release);
    REVOKE_TARGET.store(target.0, Ordering::Release);
    let t1 = sys::thread_create(
        Handle::SELF_PROC,
        derive_worker as *const () as usize,
        stack_top(0),
        0,
    )?;
    close(t1);
    let t2 = sys::thread_create(
        Handle::SELF_PROC,
        revoke_worker as *const () as usize,
        stack_top(1),
        0,
    )?;
    close(t2);
    Ok(())
}

fn finish_race() -> bool {
    if !wait_for(&REVOKE_DONE) {
        return false;
    }
    if REVOKE_OK.load(Ordering::Acquire) == 0 {
        return false;
    }
    let start = now_ns();
    while COUNT.load(Ordering::Acquire) < TRANSFERS {
        if now_ns().saturating_sub(start) > WAIT_NS {
            return false;
        }
        sys::yield_now();
    }
    true
}

fn use_children() -> (usize, usize, usize, usize) {
    let count = COUNT.load(Ordering::Acquire).min(MAX_CHILDREN);
    let mut usable = 0usize;
    let mut eperm = 0usize;
    let mut eacces = 0usize;
    let mut other = 0usize;
    for index in 0..count {
        let raw = CHILDREN[index].load(Ordering::Acquire);
        if raw == u32::MAX {
            continue;
        }
        let child = Handle(raw);
        match map_read(child) {
            Ok(address) => {
                usable = usable.saturating_add(1);
                let _ = sys::unmap(Handle::SELF_PROC, address, PAGE);
            }
            Err(Errno::EPERM) => eperm = eperm.saturating_add(1),
            Err(Errno::EACCES) => eacces = eacces.saturating_add(1),
            Err(_) => other = other.saturating_add(1),
        }
        close(child);
    }
    (usable, eperm, eacces, other)
}

fn race_shape(source: Handle, target: Handle) -> Result<(), &'static str> {
    start_race(0, source, target).map_err(|_| "race threads")?;
    if !finish_race() {
        return Err("race did not finish");
    }
    let (usable, eperm, eacces, other) = use_children();
    println!(
        "handles: race children={} eperm={} eacces={} usable={} other={}",
        COUNT.load(Ordering::Acquire),
        eperm,
        eacces,
        usable,
        other
    );
    if usable != 0 || other != 0 || eperm == 0 {
        return Err("race child usable");
    }
    Ok(())
}

fn direct_case() -> Result<(), &'static str> {
    let parent = memobj_create().map_err(|_| "direct memobj")?;
    let outcome = race_shape(parent, parent);
    close(parent);
    outcome
}

fn closed_case() -> Result<(), &'static str> {
    let parent = memobj_create().map_err(|_| "closed memobj")?;
    let derive = Rights::MAP
        .union(Rights::READ)
        .union(Rights::TRANSFER)
        .union(Rights::DUP);
    let intermediate = dup(parent, derive).map_err(|_| "closed intermediate")?;
    let leaf = dup(intermediate, derive).map_err(|_| "closed leaf")?;
    close(intermediate);
    let outcome = {
        // The race derives from the leaf while the intermediate is closed; the revoke targets
        // the root, so only the ancestor chain can bind the leaf's children.
        start_race(0, leaf, parent)
            .map_err(|_| "closed threads")
            .and_then(|()| {
                if !finish_race() {
                    return Err("closed did not finish");
                }
                let (usable, eperm, eacces, other) = use_children();
                println!(
                    "handles: closed children={} eperm={} eacces={} usable={} other={}",
                    COUNT.load(Ordering::Acquire),
                    eperm,
                    eacces,
                    usable,
                    other
                );
                if usable != 0 || other != 0 || eperm == 0 {
                    return Err("closed child usable");
                }
                Ok(())
            })
    };
    close(leaf);
    close(parent);
    outcome
}

fn queued_case() -> Result<(), &'static str> {
    let parent = memobj_create().map_err(|_| "queued memobj")?;
    start_race(1, parent, parent).map_err(|_| "queued threads")?;
    if !finish_race() {
        return Err("queued did not finish");
    }
    let count = COUNT.load(Ordering::Acquire).min(MAX_CHILDREN);
    let (sender, receiver) = duplex_pair().map_err(|_| "queued pair")?;
    let mut installed = 0usize;
    let mut usable = 0usize;
    let mut other = 0usize;
    for index in 0..count {
        let raw = CHILDREN[index].load(Ordering::Acquire);
        if raw == u32::MAX {
            continue;
        }
        let queued = Handle(raw);
        let mut control = [0u8; 64];
        let control_len = control_one(queued, &mut control);
        let sent = sendmsg(sender, b"q", &control[..control_len]);
        close(queued);
        if sent.is_err() {
            continue;
        }
        let mut byte = [0u8; 1];
        let mut received = [0u8; 64];
        match recvmsg(receiver, &mut byte, &mut received) {
            Ok((1, message)) if message.handle_count == 1 => {
                installed = installed.saturating_add(1);
                let raw_installed = get_u32(&received, CMSG_BYTES).unwrap_or(u32::MAX);
                let installed_handle = Handle(raw_installed);
                match map_read(installed_handle) {
                    Ok(address) => {
                        usable = usable.saturating_add(1);
                        let _ = sys::unmap(Handle::SELF_PROC, address, PAGE);
                    }
                    Err(_) => {}
                }
                close(installed_handle);
            }
            _ => other = other.saturating_add(1),
        }
    }
    close(sender);
    close(receiver);
    close(parent);
    println!(
        "handles: queued children={count} installed={installed} usable={usable} other={other}"
    );
    if installed == 0 || usable != 0 || other != 0 {
        return Err("queued child usable");
    }
    Ok(())
}

fn fail(message: &str) -> i32 {
    println!("handles: failed {message}");
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("handles: start");
    if let Err(message) = direct_case() {
        return fail(message);
    }
    if let Err(message) = closed_case() {
        return fail(message);
    }
    if let Err(message) = queued_case() {
        return fail(message);
    }
    println!("handles: done");
    0
}
