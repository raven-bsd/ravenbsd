// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K543 readiness contract fixture.
//!
//! Three rows exercise the event-core contract against NetBSD `sys_select.c` semantics:
//!
//! * lost wake: a signaller reacts to a round marker written immediately before `EVQ_WAIT`, so
//!   the signal lands between the entry scan and the sleep. `wait_once` re-checks readiness under
//!   the wait lock (`selrecord`), so the waiter must never time out.
//! * HUP re-poll: closing the pipe writer leaves the read end at `READABLE|HUP`; a second wait
//!   must still report `HUP` (`seltrue` is level-true, never cached).
//! * EINTR re-evaluation: `THREAD_KICK` interrupts a blocked wait; after the event is signalled the
//!   retry must observe the live level (`sel_setevents`/`SEL_RESET`).
//! * proc-wait: two forked children exit in reverse order; the parent blocks for the later child
//!   through `PROC_WAIT_EX(P_PID, pid)` and must get that child's pid and POSIX exit status.
//!   A SIGCHLD from the earlier child and a `THREAD_KICK` must not lose the wait or turn it into
//!   EINTR.
//!
//! The fixture is a guest proof; the mutating negative checks run against the kernel event core.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscall for THREAD_KICK and the thread stack; each block has its SAFETY note.

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_KICK};
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_proto_fs::OpenFlags;
use raven_proto_net::socket::{Call, Response, SocketRequest};
use raven_rt::event::{EventQueue, EvqEvent, HUP, READABLE, SIGNALED, event_create, event_signal};
use raven_rt::pipe::pipe;
use raven_rt::spawn::{SpawnOptions, spawn as spawn_process, wait_process};
use raven_rt::thread;
use raven_rt::{entry, handle_close, nanosleep, println, sys};
use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

entry!(main);

/// Rounds where the signal is fired between the entry scan and the sleep.
const LOST_WAKE_ROUNDS: u32 = 2000;
/// Registrations scanned before the enqueue. The target event is first, so the signal fired
/// while the scan walks these later registrations lands after the target's readiness evaluation
/// and before `wait_once` publishes the waiter.
const SCAN_FILLER: usize = 255;
/// Largest signaller spin delay after the round marker, in `spin_loop` iterations.
const MAX_SIGNAL_DELAY: u32 = 20_000;
/// Timeout for one lost-wake round; a lost wake shows as a zero result.
const ROUND_TIMEOUT_NS: u64 = 200_000_000;
/// Timeout for one EINTR retry.
const EINTR_TIMEOUT_NS: u64 = 300_000_000;
/// Delay before kicking the blocked waiter.
const KICK_DELAY_NS: u64 = 50_000_000;
/// User thread stack size.
const STACK_SIZE: usize = 64 * 1024;
/// `THREAD_KICK` bit: an arbitrary non-zero kick makes the next interruptible wait return EINTR.
const KICK_BIT: usize = 1;

/// Raw `PROC_FORK` pre-enum dispatch number.
const PROC_FORK: usize = 0x39;
/// Raw `PROC_HANDLE_INFO` pre-enum dispatch number.
const PROC_HANDLE_INFO: usize = 0x3e;
/// Raw `PROC_WAIT_EX` pre-enum dispatch number.
const PROC_WAIT_EX: usize = 0x48;
/// `wait6` id type: a single process id.
const P_PID: usize = 1;
/// Child A exits after this long (and must be waited for first).
const CHILD_A_NS: u64 = 200_000_000;
/// Child B exits after this long, before child A.
const CHILD_B_NS: u64 = 50_000_000;
/// Child A exit code.
const CHILD_A_CODE: i32 = 23;
/// Child B exit code.
const CHILD_B_CODE: i32 = 42;
/// Delay between arming the waiter and the parent kick.
const PROC_KICK_DELAY_NS: u64 = 20_000_000;
/// How long to wait for the worker to enter the blocking wait.
const PROC_ARM_POLLS: u32 = 1000;
/// Poll interval while waiting for the worker to arm.
const PROC_ARM_NS: u64 = 1_000_000;

#[repr(align(16))]
struct ChildStack(#[allow(dead_code)] [u8; 16 * 1024]);

static mut CHILD_STACK_A: ChildStack = ChildStack([0; 16 * 1024]);
static mut CHILD_STACK_B: ChildStack = ChildStack([0; 16 * 1024]);

static PROC_WAIT_ARMED: AtomicBool = AtomicBool::new(false);
static PROC_WAIT_FAIL: AtomicU32 = AtomicU32::new(0);
static PROC_WAIT_FIRST_PID: AtomicI32 = AtomicI32::new(0);
static PROC_WAIT_FIRST_STATUS: AtomicI32 = AtomicI32::new(0);
static PROC_WAIT_SECOND_PID: AtomicI32 = AtomicI32::new(0);
static PROC_WAIT_SECOND_STATUS: AtomicI32 = AtomicI32::new(0);

static ROUND: AtomicU32 = AtomicU32::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static SIGNAL_EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static SIGNAL_ERROR: AtomicU32 = AtomicU32::new(0);

static EINTR_QUEUE: AtomicU32 = AtomicU32::new(u32::MAX);
static EINTR_EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static EINTR_SAW: AtomicU32 = AtomicU32::new(0);
static EINTR_DONE: AtomicI32 = AtomicI32::new(0);

type R = Result<(), &'static str>;

fn signaller(_: usize) {
    let event = Handle(SIGNAL_EVENT.load(Ordering::Acquire));
    let mut seen = u32::MAX;
    while !STOP.load(Ordering::Acquire) {
        let round = ROUND.load(Ordering::Acquire);
        if round != seen {
            seen = round;
            // Sweep the delay across the entry scan: some rounds fire before `collect_ready`,
            // some in the scan tail after the target was evaluated, some after the enqueue.
            for _ in 0..(round % MAX_SIGNAL_DELAY) {
                core::hint::spin_loop();
            }
            if event_signal(event, 1).is_err() {
                SIGNAL_ERROR.store(1, Ordering::Release);
            }
        }
        core::hint::spin_loop();
    }
}

fn lost_wake() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let event = event_create().map_err(|_| "event")?;
    queue.add(event, SIGNALED, 1).map_err(|_| "add")?;
    // Fill the registration table with idle user events. They are never signalled, so only the
    // target event can make the queue ready; they only widen the entry scan.
    for _ in 0..SCAN_FILLER {
        let filler = event_create().map_err(|_| "filler")?;
        queue.add(filler, SIGNALED, 0).map_err(|_| "add filler")?;
    }
    SIGNAL_EVENT.store(event.0, Ordering::Release);
    let worker = thread::spawn(signaller, 0, STACK_SIZE).map_err(|_| "thread")?;

    let mut output = [EvqEvent::default(); 1];
    let mut timeouts = 0u32;
    for round in 1..=LOST_WAKE_ROUNDS {
        ROUND.store(round, Ordering::Release);
        match queue.wait(&mut output, ROUND_TIMEOUT_NS) {
            Ok(1) if output[0].cookie == 1 && output[0].events & SIGNALED != 0 => {}
            Ok(0) => {
                timeouts = timeouts.saturating_add(1);
                break;
            }
            _ => break,
        }
    }
    STOP.store(true, Ordering::Release);
    worker.join().map_err(|_| "join")?;
    if SIGNAL_ERROR.load(Ordering::Acquire) != 0 {
        return Err("signaller");
    }
    if timeouts != 0 {
        return Err("lost wake");
    }
    println!(
        "readiness: lost-wake rounds={} timeouts=0 ok",
        LOST_WAKE_ROUNDS
    );
    Ok(())
}

fn hup_repoll() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let (read, write) = pipe().map_err(|_| "pipe")?;
    queue.add(read, READABLE | HUP, 2).map_err(|_| "add pipe")?;
    handle_close(write).map_err(|_| "close writer")?;

    let mut output = [EvqEvent::default(); 1];
    let first = queue
        .wait(&mut output, ROUND_TIMEOUT_NS)
        .map_err(|_| "wait first")?;
    if first != 1 || output[0].events & HUP == 0 {
        return Err("first not hup");
    }
    let first_events = output[0].events;
    let second = queue
        .wait(&mut output, ROUND_TIMEOUT_NS)
        .map_err(|_| "wait second")?;
    if second != 1 || output[0].events & HUP == 0 {
        return Err("second not hup");
    }
    println!(
        "readiness: hup-repoll first=0x{:x} second=0x{:x} ok",
        first_events, output[0].events
    );
    Ok(())
}

fn eintr_waiter(_: usize) {
    let queue = EINTR_QUEUE.load(Ordering::Acquire);
    let mut output = [EvqEvent::default(); 1];
    let mut saw = 0u32;
    for _ in 0..10 {
        // SAFETY: EVQ_WAIT reads one output record from the live stack array and validates the
        // queue handle; the timeout is a scalar.
        let raw = unsafe {
            sys::syscall4(
                Syscall::EVQ_WAIT as usize,
                queue as usize,
                output.as_mut_ptr() as usize,
                1,
                EINTR_TIMEOUT_NS as usize,
            )
        };
        if raw == Errno::EINTR.to_ret() {
            saw = saw.saturating_add(1);
            EINTR_SAW.store(saw, Ordering::Release);
            continue;
        }
        if raw == 1 && output[0].cookie == 3 {
            EINTR_DONE.store(1, Ordering::Release);
            return;
        }
        EINTR_DONE.store(-1, Ordering::Release);
        return;
    }
    EINTR_DONE.store(-2, Ordering::Release);
}

fn eintr_reevaluate() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let event = event_create().map_err(|_| "event")?;
    queue.add(event, SIGNALED, 3).map_err(|_| "add")?;
    EINTR_QUEUE.store(queue.handle().0, Ordering::Release);
    EINTR_EVENT.store(event.0, Ordering::Release);
    let worker = thread::spawn(eintr_waiter, 0, STACK_SIZE).map_err(|_| "thread")?;

    nanosleep(KICK_DELAY_NS).map_err(|_| "sleep")?;
    let thread_handle = worker.handle().ok_or("no thread handle")?;
    // SAFETY: THREAD_KICK takes the thread handle and a scalar bit mask; the handle came from
    // THREAD_CREATE and carries SIGNAL.
    let kicked = unsafe { sys::syscall2(THREAD_KICK, thread_handle.0 as usize, KICK_BIT) };
    if kicked < 0 {
        return Err("kick");
    }
    event_signal(event, 1).map_err(|_| "signal")?;
    worker.join().map_err(|_| "join")?;

    let saw = EINTR_SAW.load(Ordering::Acquire);
    let done = EINTR_DONE.load(Ordering::Acquire);
    if saw == 0 || done != 1 {
        return Err("eintr");
    }
    println!("readiness: eintr retries={} result=1 ok", saw);
    Ok(())
}

fn child_stack_top(stack: *mut ChildStack) -> usize {
    stack as usize + core::mem::size_of::<ChildStack>()
}

extern "C" fn proc_wait_child_a(_: usize) -> ! {
    let _ = nanosleep(CHILD_A_NS);
    // SAFETY: EXIT terminates only the forked child process.
    unsafe { sys::syscall1(Syscall::EXIT as usize, CHILD_A_CODE as usize) };
    loop {
        core::hint::spin_loop();
    }
}

extern "C" fn proc_wait_child_b(_: usize) -> ! {
    let _ = nanosleep(CHILD_B_NS);
    // SAFETY: EXIT terminates only the forked child process.
    unsafe { sys::syscall1(Syscall::EXIT as usize, CHILD_B_CODE as usize) };
    loop {
        core::hint::spin_loop();
    }
}

fn fork_wait_child(
    entry: extern "C" fn(usize) -> !,
    stack_top: usize,
) -> Result<(usize, i32), &'static str> {
    // SAFETY: PROC_FORK takes the self-process handle and a zero flag word; the kernel copies the
    // address space and returns a process handle owned by the caller.
    let handle = unsafe { sys::syscall2(PROC_FORK, Handle::SELF_PROC.0 as usize, 0) };
    if handle < 0 {
        return Err("fork");
    }
    // SAFETY: PROC_HANDLE_INFO takes a valid process handle and returns its pid.
    let pid = unsafe { sys::syscall1(PROC_HANDLE_INFO, handle as usize) };
    if pid < 0 {
        return Err("pid");
    }
    // SAFETY: THREAD_CREATE takes scalar values; the entry is in the forked text mapping and the
    // stack top is the 16-byte-aligned top of a writable static mapping.
    let created = unsafe {
        sys::syscall6(
            Syscall::THREAD_CREATE as usize,
            handle as usize,
            entry as usize,
            stack_top,
            0,
            0,
            0,
        )
    };
    if created < 0 {
        return Err("thread");
    }
    Ok((handle as usize, pid as i32))
}

fn wait_one_pid(pid: i32) -> Result<i32, u32> {
    let mut status: i32 = 0;
    // SAFETY: PROC_WAIT_EX takes scalar selectors and a writable i32 pointer in this thread's
    // stack; P_PID selects the known child pid and options=0 makes the call blocking.
    let raw = unsafe {
        sys::syscall4(
            PROC_WAIT_EX,
            P_PID,
            pid as usize,
            0,
            &mut status as *mut i32 as usize,
        )
    };
    if raw == Errno::EINTR.to_ret() {
        return Err(1);
    }
    if raw < 0 {
        return Err(5);
    }
    if raw as i32 != pid {
        return Err(2);
    }
    Ok(status)
}

fn proc_wait_waiter(_: usize) {
    let stack_a = core::ptr::addr_of_mut!(CHILD_STACK_A);
    let stack_b = core::ptr::addr_of_mut!(CHILD_STACK_B);
    let (_handle_a, pid_a) = match fork_wait_child(proc_wait_child_a, child_stack_top(stack_a)) {
        Ok(child) => child,
        Err(_) => {
            PROC_WAIT_FAIL.store(4, Ordering::Release);
            return;
        }
    };
    let (_handle_b, pid_b) = match fork_wait_child(proc_wait_child_b, child_stack_top(stack_b)) {
        Ok(child) => child,
        Err(_) => {
            PROC_WAIT_FAIL.store(4, Ordering::Release);
            return;
        }
    };

    // Child B exits first; the parent waits for child A first. The child B SIGCHLD and the
    // parent's THREAD_KICK must not substitute EINTR or B's pid for A's wait.
    PROC_WAIT_ARMED.store(true, Ordering::Release);
    let first_status = match wait_one_pid(pid_a) {
        Ok(status) => status,
        Err(code) => {
            PROC_WAIT_FAIL.store(code, Ordering::Release);
            return;
        }
    };
    if first_status != CHILD_A_CODE << 8 {
        PROC_WAIT_FAIL.store(3, Ordering::Release);
        return;
    }
    let second_status = match wait_one_pid(pid_b) {
        Ok(status) => status,
        Err(code) => {
            PROC_WAIT_FAIL.store(code, Ordering::Release);
            return;
        }
    };
    if second_status != CHILD_B_CODE << 8 {
        PROC_WAIT_FAIL.store(3, Ordering::Release);
        return;
    }
    PROC_WAIT_FIRST_PID.store(pid_a, Ordering::Release);
    PROC_WAIT_FIRST_STATUS.store(first_status, Ordering::Release);
    PROC_WAIT_SECOND_PID.store(pid_b, Ordering::Release);
    PROC_WAIT_SECOND_STATUS.store(second_status, Ordering::Release);
}

fn proc_wait() -> R {
    PROC_WAIT_ARMED.store(false, Ordering::Release);
    PROC_WAIT_FAIL.store(0, Ordering::Release);
    let worker = thread::spawn(proc_wait_waiter, 0, STACK_SIZE).map_err(|_| "thread")?;
    let mut armed = false;
    for _ in 0..PROC_ARM_POLLS {
        if PROC_WAIT_ARMED.load(Ordering::Acquire) {
            armed = true;
            break;
        }
        nanosleep(PROC_ARM_NS).map_err(|_| "arm sleep")?;
    }
    if !armed {
        return Err("arm timeout");
    }
    nanosleep(PROC_KICK_DELAY_NS).map_err(|_| "kick delay")?;
    let handle = worker.handle().ok_or("no thread handle")?;
    // SAFETY: THREAD_KICK takes the worker thread handle and a scalar bit mask.
    let kicked = unsafe { sys::syscall2(THREAD_KICK, handle.0 as usize, KICK_BIT) };
    if kicked < 0 {
        return Err("kick");
    }
    worker.join().map_err(|_| "join")?;

    match PROC_WAIT_FAIL.load(Ordering::Acquire) {
        0 => {}
        1 => return Err("eintr"),
        2 => return Err("wrong pid"),
        3 => return Err("bad status"),
        4 => return Err("child setup"),
        _ => return Err("wait"),
    }
    let first_pid = PROC_WAIT_FIRST_PID.load(Ordering::Acquire);
    let first_status = PROC_WAIT_FIRST_STATUS.load(Ordering::Acquire);
    let second_pid = PROC_WAIT_SECOND_PID.load(Ordering::Acquire);
    let second_status = PROC_WAIT_SECOND_STATUS.load(Ordering::Acquire);
    if first_pid <= 0 || second_pid <= 0 || first_pid == second_pid {
        return Err("missing result");
    }
    println!(
        "readiness: proc-wait first_pid={} first_status={} second_pid={} second_status={} ok",
        first_pid, first_status, second_pid, second_status
    );
    Ok(())
}

struct NetHandler;

impl Handler for NetHandler {
    fn open_with(
        &mut self,
        path: &str,
        _flags: OpenFlags,
        _mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        if path.is_empty() {
            return Err(Errno::EINVAL.into());
        }
        Ok(Opened::File(1))
    }

    fn call(&mut self, _file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        if let Ok(SocketRequest::Call(call)) = SocketRequest::decode(request) {
            match call {
                Call::Bind(_) => println!("net-server: bind"),
                Call::Listen { .. } => println!("net-server: listen"),
                _ => {}
            }
        }
        let mut encoded = [0u8; 32];
        let length = Response::Ok
            .encode(&mut encoded)
            .map_err(|_| Errno::EPROTO)?;
        let destination = response.get_mut(..length).ok_or(Errno::E2BIG)?;
        destination.copy_from_slice(&encoded[..length]);
        Ok(length)
    }
}

fn net_server(_: usize) {
    let mut server = match Server::register("net") {
        Ok(server) => server,
        Err(error) => {
            println!("readiness: net-server error={}", error.0);
            return;
        }
    };
    println!("readiness: net-server ready");
    let mut handler = NetHandler;
    server.run(&mut handler);
}

fn net_listen() -> R {
    let image = raven_rt::map_initfs().map_err(|_| "net initfs")?;
    let initfs = Initfs::parse(image).map_err(|_| "net initfs parse")?;
    let child = spawn_process(
        &initfs,
        "/child",
        &SpawnOptions {
            args: &["net-child"],
            env: &[],
        },
    )
    .map_err(|_| "net child spawn")?;
    // The child exists before this thread registers `net`; its namespace snapshot is the
    // pre-registration one. The child retries until the publisher is live.
    let server = thread::spawn(net_server, 0, STACK_SIZE).map_err(|_| "net server")?;
    let code = wait_process(child.0).map_err(|_| "net child wait")?;
    core::mem::forget(server);
    if code != 0 {
        return Err("net listen child");
    }
    println!("readiness: net-listen ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    println!("readiness: start");
    let mut failed = 0;
    for (name, row) in [
        ("lost-wake", lost_wake as fn() -> R),
        ("hup-repoll", hup_repoll),
        ("eintr", eintr_reevaluate),
        ("proc-wait", proc_wait),
        ("net-listen", net_listen),
    ] {
        if let Err(reason) = row() {
            println!("readiness: failed {} {}", name, reason);
            failed += 1;
        }
    }
    if failed != 0 {
        return 1;
    }
    println!("readiness: done");
    0
}
