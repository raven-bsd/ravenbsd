// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S207 duplex cycle authorisation fixture.
//!
//! The parent and a forked child build one unreachable two-process duplex cycle: each side sends
//! its own endpoint into the peer's receive queue and then closes the external handle. The child
//! drops to uid 1000 and triggers the global collector; the fix must refuse it with `EPERM` and
//! leave both queued handles in place. The root parent then collects and both queues disappear.
//! All calls use the raw duplex ABI and the process-control syscalls, so the fixture asserts the
//! kernel contract directly.

#![no_std]
#![no_main]
#![allow(unsafe_code)]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::{handle_close, syscall6, yield_now};

entry!(main);

/// Raw `duplex_pair(out: *mut [u32; 2], flags: u32) -> 0` (K208; not in the ABI crate).
const DUPLEX_PAIR: usize = 0x110;
/// Raw `duplex_sendmsg(handle: u32, msg: *mut DuplexMessage) -> bytes`.
const DUPLEX_SENDMSG: usize = 0x112;
/// Diagnostic flag: write [`Stats`] instead of creating a pair.
const DUPLEX_DIAGNOSTIC: u32 = 0x8000_0000;
/// Diagnostic flag: run one cycle-collection pass before writing [`Stats`].
const DUPLEX_COLLECT: u32 = 0x4000_0000;
/// `SOL_SOCKET` control level.
const SOL_SOCKET: u32 = 1;
/// `SCM_RIGHTS` control type.
const SCM_RIGHTS: u32 = 1;
/// Bytes in one control record with a single handle.
const CONTROL_BYTES: u32 = 20;
/// `proc_fork(source: Handle) -> Handle`.
const PROC_FORK: usize = 0x39;
/// `proc_setcred_ex(which, a, b, c, groups, count) -> 0`.
const PROC_SETCRED_EX: usize = 0x3B;
/// `setresuid(real, effective, saved)`.
const KIND_SETRESUID: usize = 4;
/// Unprivileged probe uid.
const PROBE_UID: u32 = 1000;

/// Shared control page address in both processes (`PROC_FORK` copies the mapping).
const CONTROL_ADDR: usize = 0x4000_0000;
/// Bytes in the shared control page.
const PAGE: usize = 4096;

const SLOT_LEFT: usize = 0;
const SLOT_RIGHT: usize = 1;
const SLOT_PARENT_QUEUED: usize = 2;
const SLOT_CHILD_QUEUED: usize = 3;
const SLOT_PARENT_CLOSED: usize = 4;
const SLOT_CHILD_DONE: usize = 5;
const SLOT_CHILD_RESULT: usize = 6;
const SLOT_CHILD_ERRNO: usize = 7;
const SLOT_CHILD_STATUS: usize = 8;
const SLOT_BASE_PAIRS: usize = 9;
const SLOT_BASE_PENDING: usize = 10;

/// Ownership counters written by the `DUPLEX_DIAGNOSTIC` query.
#[repr(C)]
struct Stats {
    pairs: u64,
    ends: u64,
    pending: u64,
    controls: u64,
}

/// User-visible duplex message header.
#[repr(C)]
struct Message {
    data: u64,
    data_len: u64,
    control: u64,
    control_len: u64,
    flags: u32,
    handle_count: u32,
}

/// One control record header followed by one handle id.
#[repr(C)]
struct ControlRecord {
    len: u32,
    level: u32,
    kind: u32,
    count: u32,
    handle: u32,
}

fn sys6(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    // SAFETY: scalar syscall arguments; the kernel validates every pointer and value.
    unsafe { syscall6(number, a0, a1, a2, a3, a4, a5) }
}

fn pair(flags: u32) -> Result<(Handle, Handle), Errno> {
    let mut handles = [0u32; 2];
    let result = sys6(
        DUPLEX_PAIR,
        handles.as_mut_ptr() as usize,
        flags as usize,
        0,
        0,
        0,
        0,
    );
    Errno::from_ret(result)?;
    Ok((Handle(handles[0]), Handle(handles[1])))
}

fn stats() -> Result<Stats, Errno> {
    let mut stats = Stats {
        pairs: 0,
        ends: 0,
        pending: 0,
        controls: 0,
    };
    let result = sys6(
        DUPLEX_PAIR,
        (&mut stats as *mut Stats) as usize,
        DUPLEX_DIAGNOSTIC as usize,
        0,
        0,
        0,
        0,
    );
    Errno::from_ret(result)?;
    Ok(stats)
}

fn collect() -> Result<Stats, Errno> {
    let mut stats = Stats {
        pairs: 0,
        ends: 0,
        pending: 0,
        controls: 0,
    };
    let result = sys6(
        DUPLEX_PAIR,
        (&mut stats as *mut Stats) as usize,
        (DUPLEX_DIAGNOSTIC | DUPLEX_COLLECT) as usize,
        0,
        0,
        0,
        0,
    );
    Errno::from_ret(result)?;
    Ok(stats)
}

fn send_handle(endpoint: Handle, handle: Handle) -> Result<usize, Errno> {
    let mut byte = 0u8;
    let mut record = ControlRecord {
        len: CONTROL_BYTES,
        level: SOL_SOCKET,
        kind: SCM_RIGHTS,
        count: 1,
        handle: handle.0,
    };
    let mut message = Message {
        data: (&mut byte as *mut u8) as u64,
        data_len: 1,
        control: (&mut record as *mut ControlRecord) as u64,
        control_len: u64::from(CONTROL_BYTES),
        flags: 0,
        handle_count: 0,
    };
    let result = sys6(
        DUPLEX_SENDMSG,
        endpoint.0 as usize,
        (&mut message as *mut Message) as usize,
        0,
        0,
        0,
        0,
    );
    Errno::from_ret(result)
}

fn proc_fork() -> Result<Handle, Errno> {
    let result = sys6(PROC_FORK, Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0);
    Errno::from_ret(result).map(|value| Handle(value as u32))
}

fn thread_create(process: Handle, entry: usize, stack_top: usize) -> Result<Handle, Errno> {
    let result = sys6(
        Syscall::THREAD_CREATE as usize,
        process.0 as usize,
        entry,
        stack_top,
        0,
        0,
        0,
    );
    Errno::from_ret(result).map(|value| Handle(value as u32))
}

fn proc_wait(process: Handle, code: &mut i32) -> Result<(), Errno> {
    let result = sys6(
        Syscall::PROC_WAIT as usize,
        process.0 as usize,
        (code as *mut i32) as usize,
        0,
        0,
        0,
        0,
    );
    Errno::from_ret(result).map(|_| ())
}

fn set_uid(uid: u32) -> Result<(), Errno> {
    let result = sys6(
        PROC_SETCRED_EX,
        KIND_SETRESUID,
        uid as usize,
        uid as usize,
        uid as usize,
        0,
        0,
    );
    Errno::from_ret(result).map(|_| ())
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    let result = sys6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0);
    Errno::from_ret(result).map(|value| Handle(value as u32))
}

fn map_shared(process: Handle, object: Handle) -> Result<(), Errno> {
    let result = sys6(
        Syscall::MAP as usize,
        process.0 as usize,
        object.0 as usize,
        0,
        CONTROL_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    );
    Errno::from_ret(result).map(|_| ())
}

fn read64(slot: usize) -> u64 {
    // SAFETY: the shared control page is mapped read/write for this fixture and `slot` is one of
    // the fixed low slots below `PAGE / 8`.
    unsafe { core::ptr::read_volatile((CONTROL_ADDR + slot * 8) as *const u64) }
}

fn write64(slot: usize, value: u64) {
    // SAFETY: the shared control page is mapped read/write for this fixture and `slot` is one of
    // the fixed low slots below `PAGE / 8`.
    unsafe { core::ptr::write_volatile((CONTROL_ADDR + slot * 8) as *mut u64, value) }
}

fn wait_flag(slot: usize) -> bool {
    for _ in 0..2_000_000 {
        if read64(slot) == 1 {
            return true;
        }
        yield_now();
    }
    false
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global duplex_cycle_child_entry
    .type duplex_cycle_child_entry, @function
duplex_cycle_child_entry:
    call duplex_cycle_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size duplex_cycle_child_entry, . - duplex_cycle_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global duplex_cycle_child_entry
    .type duplex_cycle_child_entry, %function
duplex_cycle_child_entry:
    bl duplex_cycle_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size duplex_cycle_child_entry, . - duplex_cycle_child_entry
"#
);

unsafe extern "C" {
    fn duplex_cycle_child_entry();
}

/// Child thread: queue the right endpoint, close both inherited handles, then trigger collection
/// as uid 1000 and publish the result.
#[unsafe(no_mangle)]
pub extern "C" fn duplex_cycle_child_body() -> i32 {
    let left = Handle(read64(SLOT_LEFT) as u32);
    let right = Handle(read64(SLOT_RIGHT) as u32);

    if set_uid(PROBE_UID) != Ok(()) {
        write64(SLOT_CHILD_STATUS, 1);
        write64(SLOT_CHILD_DONE, 1);
        return 1;
    }

    if send_handle(right, right) != Ok(1) {
        write64(SLOT_CHILD_STATUS, 2);
        write64(SLOT_CHILD_DONE, 1);
        return 2;
    }
    write64(SLOT_CHILD_QUEUED, 1);

    if !wait_flag(SLOT_PARENT_QUEUED) {
        write64(SLOT_CHILD_STATUS, 3);
        write64(SLOT_CHILD_DONE, 1);
        return 3;
    }
    if handle_close(left).is_err() || handle_close(right).is_err() {
        write64(SLOT_CHILD_STATUS, 4);
        write64(SLOT_CHILD_DONE, 1);
        return 4;
    }

    if !wait_flag(SLOT_PARENT_CLOSED) {
        write64(SLOT_CHILD_STATUS, 5);
        write64(SLOT_CHILD_DONE, 1);
        return 5;
    }

    let result = collect();
    write64(
        SLOT_CHILD_RESULT,
        match result {
            Ok(_) => 0,
            Err(error) => error.to_ret() as u64,
        },
    );
    write64(
        SLOT_CHILD_ERRNO,
        match result {
            Ok(_) => 0,
            Err(error) => error.0 as u64,
        },
    );
    write64(SLOT_CHILD_STATUS, 0);
    write64(SLOT_CHILD_DONE, 1);
    0
}

fn cycle() -> Result<bool, Errno> {
    let baseline = stats()?;
    let (left, right) = pair(0)?;

    let control = memobj_create(PAGE)?;
    map_shared(Handle::SELF_PROC, control)?;
    write64(SLOT_LEFT, u64::from(left.0));
    write64(SLOT_RIGHT, u64::from(right.0));
    write64(SLOT_BASE_PAIRS, baseline.pairs);
    write64(SLOT_BASE_PENDING, baseline.pending);

    let child = proc_fork()?;
    thread_create(
        child,
        duplex_cycle_child_entry as *const () as usize,
        stack_top(),
    )?;

    // Parent side: queue its left endpoint into the right side's queue, then release the
    // external handles only after the child queued its side.
    if send_handle(left, left) != Ok(1) {
        return Ok(false);
    }
    write64(SLOT_PARENT_QUEUED, 1);
    if !wait_flag(SLOT_CHILD_QUEUED) {
        return Ok(false);
    }
    handle_close(right)?;
    handle_close(left)?;
    write64(SLOT_PARENT_CLOSED, 1);
    if !wait_flag(SLOT_CHILD_DONE) {
        return Ok(false);
    }

    let mut code = -1i32;
    proc_wait(child, &mut code)?;
    let child_status = read64(SLOT_CHILD_STATUS);
    let child_errno = read64(SLOT_CHILD_ERRNO);
    let child_result = read64(SLOT_CHILD_RESULT);

    let intact = stats()?;
    let denied = child_status == 0
        && child_result == Errno::EPERM.to_ret() as u64
        && child_errno == u64::from(Errno::EPERM.0)
        && intact.pairs == baseline.pairs.saturating_add(1)
        && intact.pending == baseline.pending.saturating_add(2);
    println!(
        "duplex-cycle: denied={} errno={} pairs={} pending={}",
        u8::from(denied),
        child_errno,
        intact.pairs.saturating_sub(baseline.pairs),
        intact.pending.saturating_sub(baseline.pending),
    );

    let collected = collect()?;
    let clean = collected.pairs == baseline.pairs && collected.pending == baseline.pending;
    println!(
        "duplex-cycle: collected={} pairs={} pending={}",
        u8::from(clean),
        collected.pairs.saturating_sub(baseline.pairs),
        collected.pending.saturating_sub(baseline.pending),
    );

    Ok(denied && clean && code == 0)
}

fn main(_args: raven_rt::Args) -> i32 {
    match cycle() {
        Ok(true) => {
            println!("duplex-cycle: ok");
            0
        }
        Ok(false) => {
            println!("duplex-cycle: FAIL checks");
            1
        }
        Err(error) => {
            println!("duplex-cycle: error {error:?}");
            1
        }
    }
}
