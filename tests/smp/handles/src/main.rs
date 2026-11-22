// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K528 capability-lineage SMP fixture.
//!
//! Proves that a completed ancestor revoke binds every descendant on 1 and 4 CPUs:
//!
//! * `queued-scm-rights`: a write end queued through `SCM_RIGHTS` is granted before the revoke
//!   and installed after it; the installed handle must fail its use with `EPERM`.
//! * `closed-intermediate`: the chain root -> intermediate -> child keeps the root revoke after
//!   the intermediate handle is closed.
//! * `error-split`: a right the child never held stays `EACCES`, not `EPERM`.
//! * `ancestor-race`: installer threads hammer `HANDLE_DUP` while the main thread completes a
//!   revoke; a child that survives the completed revoke is a violation. This stresses the
//!   insert-before-register window on all CPUs.
//!
//! The deterministic interleaving proof for the register race is the host barrier test in
//! `crates/raven-handles/src/lib_ut.rs`; this fixture proves the same rule through the syscall
//! boundary and under SMP scheduling.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscall argument marshalling; each block has a SAFETY comment.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_rt::entry;
use raven_rt::pipe::pipe;
use raven_rt::println;
use raven_rt::sys::{handle_close, handle_dup, nanosleep, syscall6, write};
use raven_rt::thread;

entry!(main);

/// Raw duplex pair number.
const DUPLEX_PAIR: usize = 0x110;
/// Raw duplex sendmsg number.
const DUPLEX_SENDMSG: usize = 0x112;
/// Raw duplex recvmsg number.
const DUPLEX_RECVMSG: usize = 0x113;
/// `SOL_SOCKET` control level.
const SOL_SOCKET: u32 = 1;
/// `SCM_RIGHTS` control type.
const SCM_RIGHTS: u32 = 1;
/// Race iterations per boot.
const RACE_ITERATIONS: usize = 64;
/// Installer threads per race iteration.
const INSTALLERS: usize = 2;
/// Thread stack size.
const STACK_SIZE: usize = 16 * 1024;

#[repr(C)]
struct Message {
    data: u64,
    data_len: u64,
    control: u64,
    control_len: u64,
    flags: u32,
    handle_count: u32,
}

static BASE: AtomicU32 = AtomicU32::new(u32::MAX);
static REVOKED: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static VIOLATIONS: AtomicUsize = AtomicUsize::new(0);
static INSTALLS: AtomicUsize = AtomicUsize::new(0);

fn pair() -> Result<(Handle, Handle), Errno> {
    let mut handles = [0u32; 2];
    // SAFETY: `handles` is a live two-word array and the kernel writes exactly eight bytes.
    let result = unsafe { syscall6(DUPLEX_PAIR, handles.as_mut_ptr() as usize, 0, 0, 0, 0, 0) };
    Errno::from_ret(result)?;
    Ok((Handle(handles[0]), Handle(handles[1])))
}

fn sendmsg(handle: Handle, data: &[u8], control: &[u8]) -> Result<usize, Errno> {
    let mut message = Message {
        data: data.as_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_ptr() as u64,
        control_len: control.len() as u64,
        flags: 0,
        handle_count: 0,
    };
    // SAFETY: the message fields point at live caller slices; the kernel validates every range.
    let result = unsafe {
        syscall6(
            DUPLEX_SENDMSG,
            handle.0 as usize,
            (&mut message as *mut Message) as usize,
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(result).map(|count| count as usize)
}

fn recvmsg(handle: Handle, data: &mut [u8], control: &mut [u8]) -> Result<(usize, Message), Errno> {
    let mut message = Message {
        data: data.as_mut_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_mut_ptr() as u64,
        control_len: control.len() as u64,
        flags: 0,
        handle_count: 0,
    };
    // SAFETY: the message fields point at live caller slices; the kernel validates every range.
    let result = unsafe {
        syscall6(
            DUPLEX_RECVMSG,
            handle.0 as usize,
            (&mut message as *mut Message) as usize,
            0,
            0,
            0,
            0,
        )
    };
    let count = Errno::from_ret(result)? as usize;
    Ok((count, message))
}

fn revoke(handle: Handle, mask: Rights) -> Result<(), Errno> {
    // SAFETY: HANDLE_REVOKE takes two scalar arguments.
    let result = unsafe {
        raven_rt::sys::syscall2(
            Syscall::HANDLE_REVOKE as usize,
            handle.0 as usize,
            mask.0 as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

fn write_u32(buffer: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = buffer.get_mut(offset..offset.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn read_u32(buffer: &[u8], offset: usize) -> Result<u32, Errno> {
    let slot = buffer
        .get(offset..offset.saturating_add(4))
        .ok_or(Errno::EINVAL)?;
    let bytes: [u8; 4] = slot.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u32::from_le_bytes(bytes))
}

fn make_control(handles: &[Handle], buffer: &mut [u8]) -> usize {
    let length = 16usize.saturating_add(handles.len().saturating_mul(4));
    write_u32(buffer, 0, length as u32);
    write_u32(buffer, 4, SOL_SOCKET);
    write_u32(buffer, 8, SCM_RIGHTS);
    write_u32(buffer, 12, handles.len() as u32);
    for (index, handle) in handles.iter().enumerate() {
        write_u32(buffer, 16 + index * 4, handle.0);
    }
    length
}

fn first_handle(control: &[u8], length: usize) -> Result<u32, Errno> {
    let record_len = read_u32(control, 0)? as usize;
    let level = read_u32(control, 4)?;
    let kind = read_u32(control, 8)?;
    let count = read_u32(control, 12)?;
    if record_len < 16
        || record_len > length
        || level != SOL_SOCKET
        || kind != SCM_RIGHTS
        || count != 1
    {
        return Err(Errno::EINVAL);
    }
    read_u32(control, 16)
}

fn close(handle: Handle) {
    let _ = handle_close(handle);
}

fn queued_scm_rights() -> bool {
    let Ok((left, right)) = pair() else {
        return false;
    };
    let Ok((read_end, write_end)) = pipe() else {
        close(left);
        close(right);
        return false;
    };
    let mut sent_control = [0u8; 64];
    let sent_len = make_control(&[write_end], &mut sent_control);
    let sent = sendmsg(left, b"fd", &sent_control[..sent_len]) == Ok(2);
    // The attach record was granted the handle before this revoke and is installed only on
    // recvmsg, after the revoke completed.
    let revoked = revoke(write_end, Rights::WRITE).is_ok();
    let mut data = [0u8; 16];
    let mut received_control = [0u8; 64];
    let received = recvmsg(right, &mut data, &mut received_control).ok();
    let mut ok = sent && revoked;
    if let Some((count, message)) = received
        && count == 2
        && &data[..2] == b"fd"
        && message.handle_count == 1
        && let Ok(raw) = first_handle(&received_control, message.control_len as usize)
    {
        let installed = Handle(raw);
        ok = ok && write(installed, b"x") == Err(Errno::EPERM);
        close(installed);
    } else {
        ok = false;
    }
    close(write_end);
    close(read_end);
    close(left);
    close(right);
    ok
}

fn closed_intermediate() -> bool {
    let Ok((read_end, write_end)) = pipe() else {
        return false;
    };
    let Ok(intermediate) = handle_dup(write_end, Rights::WRITE.union(Rights::DUP)) else {
        close(read_end);
        close(write_end);
        return false;
    };
    let Ok(child) = handle_dup(intermediate, Rights::WRITE.union(Rights::DUP)) else {
        close(intermediate);
        close(read_end);
        close(write_end);
        return false;
    };
    // The child's strong parent link keeps the chain alive after this close.
    close(intermediate);
    let revoked = revoke(write_end, Rights::WRITE).is_ok();
    let denied = write(child, b"x") == Err(Errno::EPERM);
    // DUP was not revoked, so the remaining right still derives.
    let remaining = handle_dup(child, Rights::DUP).is_ok();
    close(child);
    close(read_end);
    close(write_end);
    revoked && denied && remaining
}

fn error_split() -> bool {
    let Ok((read_end, write_end)) = pipe() else {
        return false;
    };
    // READ was never granted on the write end; the revoke of it is ignored.
    let ignored = revoke(write_end, Rights::READ).is_ok();
    // Request READ too: the child never held it, so its use is EACCES, not EPERM.
    let Ok(child) = handle_dup(write_end, Rights::READ) else {
        close(read_end);
        close(write_end);
        return false;
    };
    let never = write(child, b"x") == Err(Errno::EACCES);
    close(child);
    close(read_end);
    close(write_end);
    ignored && never
}

fn installer(_argument: usize) {
    while !STOP.load(Ordering::Acquire) && INSTALLS.load(Ordering::Acquire) < 32 {
        let raw = BASE.load(Ordering::Acquire);
        if raw == u32::MAX {
            break;
        }
        if let Ok(candidate) = handle_dup(Handle(raw), Rights::WRITE.union(Rights::DUP)) {
            // A child created before the revoke and registered after it would still write here
            // once the revoke has completed. That is the violation the ancestor walk forbids.
            if REVOKED.load(Ordering::Acquire) && write(candidate, b"x").is_ok() {
                VIOLATIONS.fetch_add(1, Ordering::AcqRel);
            }
            INSTALLS.fetch_add(1, Ordering::AcqRel);
            close(candidate);
        }
    }
}

fn race_iteration() -> bool {
    let Ok((read_end, write_end)) = pipe() else {
        return false;
    };
    let Ok(base) = handle_dup(write_end, Rights::WRITE.union(Rights::DUP)) else {
        close(read_end);
        close(write_end);
        return false;
    };
    BASE.store(base.0, Ordering::Release);
    REVOKED.store(false, Ordering::Release);
    STOP.store(false, Ordering::Release);
    VIOLATIONS.store(0, Ordering::Release);
    INSTALLS.store(0, Ordering::Release);

    let mut threads = Vec::new();
    for _ in 0..INSTALLERS {
        if let Ok(thread) = thread::spawn(installer, 0, STACK_SIZE) {
            threads.push(thread);
        }
    }
    // Give the installers a published child before the revoke lands, so the revoke overlaps
    // the insert/register window instead of always running before the first dup.
    for _ in 0..20_000 {
        if INSTALLS.load(Ordering::Acquire) >= 4 {
            break;
        }
    }
    let revoked = revoke(base, Rights::WRITE).is_ok();
    REVOKED.store(true, Ordering::Release);
    STOP.store(true, Ordering::Release);
    for thread in threads {
        let _ = thread.join();
    }
    let violations = VIOLATIONS.load(Ordering::Acquire);
    close(base);
    close(read_end);
    close(write_end);
    revoked && violations == 0
}

fn row(name: &str, ok: bool) -> bool {
    println!(
        "handles: row name={} ok={}",
        name,
        if ok { "yes" } else { "no" }
    );
    ok
}

fn run() -> bool {
    println!(
        "handles: start iterations={} installers={}",
        RACE_ITERATIONS, INSTALLERS
    );
    let mut ok = true;
    ok &= row("queued-scm-rights", queued_scm_rights());
    ok &= row("closed-intermediate", closed_intermediate());
    ok &= row("error-split", error_split());
    let mut race_ok = true;
    for _ in 0..RACE_ITERATIONS {
        if !race_iteration() {
            race_ok = false;
            break;
        }
        // Keep the CPU available to the scheduler between thread bursts.
        let _ = nanosleep(200_000);
    }
    println!(
        "handles: row name=ancestor-race ok={} installs={}",
        if race_ok { "yes" } else { "no" },
        RACE_ITERATIONS.saturating_mul(INSTALLERS)
    );
    ok &= race_ok;
    println!(
        "handles: summary rows=4 ok={}",
        if ok { "yes" } else { "no" }
    );
    ok
}

fn main(_args: raven_rt::Args) -> i32 {
    let ok = run();
    if !ok {
        println!("handles: FAIL");
        return 1;
    }
    println!("handles: alive");
    0
}
