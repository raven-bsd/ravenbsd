// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S38 fixture: event registrations die with the handle that keyed them.
//!
//! The fixture drives the four acceptance cases from the ticket:
//!
//! 1. register, close the registered handle, signal the still-live object: no delivery; a new
//!    object reusing the slot inherits nothing, and the queue still delivers after a fresh
//!    registration;
//! 2. a helper thread closes the registered handle and exits; the registration is gone and the
//!    process keeps running;
//! 3. a child process registers an event and exits without closing it; the process-exit hook
//!    clears the registration and the next process is unaffected;
//! 4. 1,000 register/close cycles on one long-lived queue return the registration count to
//!    baseline (the kernel prints the live count at every process exit when `s38-eventcheck` is
//!    on the command line).
//!
//! Roles: `marker` (exit 0), `leak` (register an event, exit without closing).

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscalls; every block documents its contract.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnError, SpawnOptions, spawn};
use raven_rt::sys::{self, map_initfs, syscall1, syscall2, syscall6};
use raven_rt::thread;

entry!(main);

/// `EVQ_CTL` add.
const ADD: usize = 1;
/// `SIGNALED` event bit.
const SIGNALED: usize = 8;
/// Cookie used by the closed-registration check.
const COOKIE: u64 = 0x5338_0001;
/// Registration/close cycles in the baseline test.
const CYCLES: usize = 1_000;
/// Bounded wait for a delivery that must not happen.
const WAIT_NS: u64 = 20_000_000;

/// One event record written by `EVQ_WAIT`.
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

fn evq_create() -> Result<Handle, Errno> {
    // SAFETY: EVQ_CREATE takes only the zero flags argument.
    handle(unsafe { syscall1(Syscall::EVQ_CREATE as usize, 0) })
}

fn event_create() -> Result<Handle, Errno> {
    // SAFETY: EVENT_CREATE takes only the zero flags argument.
    handle(unsafe { syscall1(Syscall::EVENT_CREATE as usize, 0) })
}

fn event_signal(event: Handle) -> Result<(), Errno> {
    // SAFETY: EVENT_SIGNAL takes one scalar handle validated by the kernel.
    result(unsafe { syscall2(Syscall::EVENT_SIGNAL as usize, event.0 as usize, 0) }).map(|_| ())
}

fn evq_ctl(
    queue: Handle,
    operation: usize,
    source: Handle,
    events: usize,
    cookie: u64,
) -> Result<(), Errno> {
    // SAFETY: EVQ_CTL follows the stable six-register syscall ABI.
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
    // SAFETY: `out` is a live writable slice and the kernel validates its length.
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

fn dup(object: Handle, rights: Rights) -> Result<Handle, Errno> {
    // SAFETY: HANDLE_DUP takes a scalar handle and rights mask; the kernel validates both.
    handle(unsafe {
        syscall2(
            Syscall::HANDLE_DUP as usize,
            object.0 as usize,
            rights.0 as usize,
        )
    })
}

fn handle_close(handle: Handle) -> Result<(), Errno> {
    sys::handle_close(handle)
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    // SAFETY: PROC_WAIT takes a process handle and a writable i32 owned by this stack frame.
    result(unsafe {
        syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    })?;
    Ok(code)
}

fn fail(message: &str) -> ! {
    println!("s38: failed {message}");
    sys::exit(1)
}

/// Spawn the fixture again with `argv[1] = role`.
fn spawn_role(initfs: &Initfs<'_>, role: &str) -> Result<Handle, SpawnError> {
    let args = ["/init", role];
    spawn(
        initfs,
        "/init",
        &SpawnOptions {
            args: &args,
            env: &[],
        },
    )
}

/// Case 1: a closed registration delivers nothing; a reused slot inherits nothing.
fn closed_registration_part() -> Result<(), &'static str> {
    let queue = evq_create().map_err(|_| "queue create")?;
    let event = event_create().map_err(|_| "event create")?;
    // The duplicate keeps the event object alive after its registered handle is closed.
    let observer = dup(event, Rights::SIGNAL).map_err(|_| "event dup")?;
    evq_ctl(queue, ADD, event, SIGNALED, COOKIE).map_err(|_| "register")?;
    handle_close(event).map_err(|_| "close registered")?;
    // The stale value is invalid; the live duplicate must not deliver through the old key.
    match event_signal(event) {
        Err(Errno::EBADF) => {}
        _ => return Err("stale event handle accepted"),
    }
    event_signal(observer).map_err(|_| "signal observer")?;
    let mut events = [EvqEvent::default(); 4];
    if evq_wait(queue, &mut events, WAIT_NS).map_err(|_| "wait observer")? != 0 {
        return Err("closed registration delivered");
    }
    // A new object reuses the closed slot; it must not inherit the old registration.
    let fresh = event_create().map_err(|_| "fresh create")?;
    event_signal(fresh).map_err(|_| "signal fresh")?;
    if evq_wait(queue, &mut events, WAIT_NS).map_err(|_| "wait fresh")? != 0 {
        return Err("reused slot inherited the registration");
    }
    // Positive control: the queue still delivers for a fresh registration.
    evq_ctl(queue, ADD, fresh, SIGNALED, COOKIE + 1).map_err(|_| "register fresh")?;
    event_signal(fresh).map_err(|_| "signal fresh 2")?;
    if evq_wait(queue, &mut events, WAIT_NS).map_err(|_| "wait fresh 2")? != 1
        || events[0].cookie != COOKIE + 1
        || events[0].events & SIGNALED as u32 == 0
    {
        return Err("queue stopped delivering");
    }
    handle_close(fresh).map_err(|_| "close fresh")?;
    handle_close(observer).map_err(|_| "close observer")?;
    handle_close(queue).map_err(|_| "close queue")?;
    println!("s38: closed registration not inherited");
    Ok(())
}

/// Helper thread: close the registered handle, then return (the runtime issues `THREAD_EXIT`).
fn close_from_thread(arg: usize) {
    let _ = handle_close(Handle(arg as u32));
}

/// Case 2: a thread other than the registrant closes the handle and exits.
fn thread_exit_part() -> Result<(), &'static str> {
    let queue = evq_create().map_err(|_| "queue create")?;
    let event = event_create().map_err(|_| "event create")?;
    let observer = dup(event, Rights::SIGNAL).map_err(|_| "event dup")?;
    evq_ctl(queue, ADD, event, SIGNALED, COOKIE + 2).map_err(|_| "register")?;
    let worker = thread::spawn(close_from_thread, event.0 as usize, 16 * 1024)
        .map_err(|_| "thread spawn")?;
    worker.join().map_err(|_| "thread join")?;
    event_signal(observer).map_err(|_| "signal observer")?;
    let mut events = [EvqEvent::default(); 4];
    if evq_wait(queue, &mut events, WAIT_NS).map_err(|_| "wait observer")? != 0 {
        return Err("registration survived the closing thread");
    }
    handle_close(observer).map_err(|_| "close observer")?;
    handle_close(queue).map_err(|_| "close queue")?;
    println!("s38: thread exit close ok");
    Ok(())
}

/// Case 3: a dying process leaves no registration behind and does not disturb the next process.
fn process_exit_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let child = spawn_role(initfs, "leak").map_err(|_| "leak spawn")?;
    match proc_wait(child) {
        Ok(0) => {}
        _ => return Err("leak wait"),
    }
    handle_close(child).map_err(|_| "leak close")?;
    // The parent's event path is unaffected: a fresh registration still delivers.
    let queue = evq_create().map_err(|_| "queue create")?;
    let event = event_create().map_err(|_| "event create")?;
    evq_ctl(queue, ADD, event, SIGNALED, COOKIE + 3).map_err(|_| "register")?;
    event_signal(event).map_err(|_| "signal")?;
    let mut events = [EvqEvent::default(); 4];
    if evq_wait(queue, &mut events, WAIT_NS).map_err(|_| "wait")? != 1 {
        return Err("parent queue lost after a child exit");
    }
    handle_close(event).map_err(|_| "close event")?;
    handle_close(queue).map_err(|_| "close queue")?;
    println!("s38: process exit hook ok");
    Ok(())
}

/// Case 4: 1,000 register/close cycles on one long-lived queue.
///
/// The queue is returned open on purpose: the final process-exit evidence line then reports its
/// live registration count, which is zero exactly when every close ran the hook.
fn cycles_part() -> Result<Handle, &'static str> {
    let queue = evq_create().map_err(|_| "queue create")?;
    let mut cycle = 0usize;
    while cycle < CYCLES {
        let event = event_create().map_err(|_| "cycle event")?;
        evq_ctl(queue, ADD, event, SIGNALED, cycle as u64).map_err(|_| "cycle register")?;
        handle_close(event).map_err(|_| "cycle close")?;
        cycle = cycle.saturating_add(1);
    }
    // The queue must still work after the churn.
    let event = event_create().map_err(|_| "final event")?;
    evq_ctl(queue, ADD, event, SIGNALED, COOKIE + 4).map_err(|_| "final register")?;
    event_signal(event).map_err(|_| "final signal")?;
    let mut events = [EvqEvent::default(); 4];
    if evq_wait(queue, &mut events, WAIT_NS).map_err(|_| "final wait")? != 1
        || events[0].cookie != COOKIE + 4
    {
        return Err("queue stopped delivering after the churn");
    }
    handle_close(event).map_err(|_| "final close")?;
    println!("s38: cycles done={CYCLES}");
    Ok(queue)
}

fn child_role(role: &str) -> i32 {
    match role {
        "marker" => 0,
        "leak" => {
            // Register an event and exit without closing it: only the process-exit hook can
            // clear the registration.
            let queue = match evq_create() {
                Ok(queue) => queue,
                Err(_) => return 1,
            };
            let event = match event_create() {
                Ok(event) => event,
                Err(_) => return 1,
            };
            if evq_ctl(queue, ADD, event, SIGNALED, COOKIE + 5).is_err() {
                return 1;
            }
            println!("s38: leak ready");
            0
        }
        _ => 2,
    }
}

fn main(args: raven_rt::Args) -> i32 {
    if let Some(role) = args.get(1) {
        return child_role(role);
    }
    let image = match map_initfs() {
        Ok(image) => image,
        Err(_) => fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail("initfs parse"),
    };

    if let Err(message) = closed_registration_part() {
        fail(message);
    }
    if let Err(message) = thread_exit_part() {
        fail(message);
    }
    if let Err(message) = process_exit_part(&initfs) {
        fail(message);
    }

    // Baseline marker: its exit prints the kernel registration counters.
    let baseline = match spawn_role(&initfs, "marker") {
        Ok(child) => child,
        Err(_) => fail("baseline spawn"),
    };
    if proc_wait(baseline).unwrap_or(-1) != 0 {
        fail("baseline wait");
    }
    let _queue = match cycles_part() {
        Ok(queue) => queue,
        Err(message) => fail(message),
    };
    // Closing marker: `removed` on this line is exactly the churn's registration count.
    let after = match spawn_role(&initfs, "marker") {
        Ok(child) => child,
        Err(_) => fail("after spawn"),
    };
    if proc_wait(after).unwrap_or(-1) != 0 {
        fail("after wait");
    }
    // The marker process handles are closed after their exit lines; they carry no registration,
    // so the `removed` evidence is unaffected.
    let _ = handle_close(baseline);
    let _ = handle_close(after);

    println!("s38: events ok");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    sys::exit(2)
}
