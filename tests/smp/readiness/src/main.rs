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
//! * sibling-exit: a sibling child exits while a worker is blocked in a pipe read, a futex wait,
//!   or an EVQ_WAIT. A no-handler `THREAD_KICK` must not turn any of them into EINTR; only the
//!   ready byte/futex/event wakes the worker.
//! * sshd-shape: a worker registers `SIG_SETUP`, blocks in EVQ_WAIT on a pipe read end, and a
//!   forked child exits. The SIGCHLD entry must run, the retry must see the pipe HUP, and
//!   `PROC_WAIT_EX` must return the child's exact status - not EINTR with a lost event.
//!
//! The fixture is a guest proof; the mutating negative checks run against the kernel event core.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscall for THREAD_KICK and the thread stack; each block has its SAFETY note.

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{SIG_SETUP, SIG_SETUP_ABS, SIGRETURN};
use raven_abi::syscall::{Syscall, THREAD_KICK};
use raven_abi::types::Handle;
use raven_abi::types::futex::{WAIT as FUTEX_WAIT, WAKE as FUTEX_WAKE};
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
/// Sibling child exit delay while a primitive wait is armed.
const SIBLING_CHILD_NS: u64 = 100_000_000;
/// Delay before sending the no-handler kick to the armed worker.
const SIBLING_KICK_NS: u64 = 20_000_000;
/// Delay after the kick, so the child exit lands before the primitive wake.
const SIBLING_WAKE_NS: u64 = 150_000_000;
/// Bounded EVQ_WAIT timeout for the sibling-exit row.
const SIBLING_EVENT_TIMEOUT_NS: u64 = 1_000_000_000;
/// Event cookie used by the sibling-exit row.
const SIBLING_EVENT_COOKIE: u64 = 7;
/// Child exit delay for the sshd-shape row.
const SSHD_CHILD_NS: u64 = 200_000_000;
/// Child exit code for the sshd-shape row.
const SSHD_CHILD_CODE: i32 = 77;
/// EVQ_WAIT timeout for the sshd-shape row.
const SSHD_TIMEOUT_NS: u64 = 1_000_000_000;
/// Event cookie for the sshd-shape pipe.
const SSHD_EVENT_COOKIE: u64 = 11;

#[repr(align(16))]
struct ChildStack(#[allow(dead_code)] [u8; 16 * 1024]);

static mut CHILD_STACK_A: ChildStack = ChildStack([0; 16 * 1024]);
static mut CHILD_STACK_B: ChildStack = ChildStack([0; 16 * 1024]);
static mut SIBLING_STACK: ChildStack = ChildStack([0; 16 * 1024]);
static mut SSHD_STACK: ChildStack = ChildStack([0; 16 * 1024]);

static PROC_WAIT_ARMED: AtomicBool = AtomicBool::new(false);
static PROC_WAIT_FAIL: AtomicU32 = AtomicU32::new(0);
static PROC_WAIT_FIRST_PID: AtomicI32 = AtomicI32::new(0);
static PROC_WAIT_FIRST_STATUS: AtomicI32 = AtomicI32::new(0);
static PROC_WAIT_SECOND_PID: AtomicI32 = AtomicI32::new(0);
static PROC_WAIT_SECOND_STATUS: AtomicI32 = AtomicI32::new(0);

static SIBLING_ARMED: AtomicBool = AtomicBool::new(false);
static SIBLING_FAIL: AtomicU32 = AtomicU32::new(0);
static SIBLING_EVENT_QUEUE: AtomicU32 = AtomicU32::new(u32::MAX);
static SIBLING_FUTEX: AtomicU32 = AtomicU32::new(0);
static NET_READY: AtomicBool = AtomicBool::new(false);
static FILE_READY: AtomicBool = AtomicBool::new(false);

static ROUND: AtomicU32 = AtomicU32::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
static SIGNAL_EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static SIGNAL_ERROR: AtomicU32 = AtomicU32::new(0);

static EINTR_QUEUE: AtomicU32 = AtomicU32::new(u32::MAX);
static EINTR_EVENT: AtomicU32 = AtomicU32::new(u32::MAX);
static EINTR_SAW: AtomicU32 = AtomicU32::new(0);
static EINTR_DONE: AtomicI32 = AtomicI32::new(0);

static SSHD_ARMED: AtomicBool = AtomicBool::new(false);
static SSHD_FAIL: AtomicU32 = AtomicU32::new(0);
static SSHD_HANDLER_RUNS: AtomicU32 = AtomicU32::new(0);
static SSHD_EINTR: AtomicU32 = AtomicU32::new(0);
static SSHD_STATUS: AtomicI32 = AtomicI32::new(0);
static SSHD_QUEUE: AtomicU32 = AtomicU32::new(u32::MAX);

#[repr(align(16))]
struct SignalContext(#[allow(dead_code)] [u8; 4096]);

static mut EINTR_SIGNAL_CONTEXT: SignalContext = SignalContext([0; 4096]);
static mut SSHD_SIGNAL_CONTEXT: SignalContext = SignalContext([0; 4096]);

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

extern "C" fn eintr_signal_handler(_: usize) -> ! {
    // SAFETY: the kernel enters this handler with a live signal context; SIGRETURN restores the
    // interrupted syscall return path for this thread.
    unsafe { sys::syscall1(SIGRETURN, 0) };
    loop {
        core::hint::spin_loop();
    }
}

fn eintr_waiter(_: usize) {
    // A raw THREAD_KICK is an interrupt only when this thread has a registered SIG_SETUP
    // handler that the kernel can run. Register the minimal absolute-context handler first.
    let context = core::ptr::addr_of_mut!(EINTR_SIGNAL_CONTEXT) as usize;
    // SAFETY: SIG_SETUP takes scalar user addresses; the context is a live 16-byte-aligned
    // static page and the entry is in the fixture text mapping.
    let setup = unsafe {
        sys::syscall4(
            SIG_SETUP,
            eintr_signal_handler as *const () as usize,
            context,
            SIG_SETUP_ABS as usize,
            0,
        )
    };
    if setup < 0 {
        EINTR_DONE.store(-3, Ordering::Release);
        return;
    }
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

extern "C" fn sibling_child(_: usize) -> ! {
    let _ = nanosleep(SIBLING_CHILD_NS);
    // SAFETY: EXIT terminates only the forked sibling process.
    unsafe { sys::syscall1(Syscall::EXIT as usize, 0) };
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

fn sibling_wait_armed() -> R {
    for _ in 0..PROC_ARM_POLLS {
        if SIBLING_ARMED.load(Ordering::Acquire) {
            return Ok(());
        }
        nanosleep(PROC_ARM_NS).map_err(|_| "arm sleep")?;
    }
    Err("arm timeout")
}

fn sibling_pipe_waiter(read_raw: usize) {
    SIBLING_ARMED.store(true, Ordering::Release);
    let mut byte = [0u8; 1];
    match sys::read(Handle(read_raw as u32), &mut byte) {
        Ok(1) if byte[0] == 0x5a => {}
        Err(Errno::EINTR) => SIBLING_FAIL.store(1, Ordering::Release),
        _ => SIBLING_FAIL.store(2, Ordering::Release),
    }
}

fn sibling_futex_waiter(_: usize) {
    SIBLING_ARMED.store(true, Ordering::Release);
    let address = core::ptr::addr_of!(SIBLING_FUTEX) as usize;
    // SAFETY: FUTEX takes a live aligned u32 address, a scalar op/value and a timeout; the
    // address is the static word the main thread wakes.
    let raw = unsafe {
        sys::syscall4(
            Syscall::FUTEX as usize,
            address,
            FUTEX_WAIT as usize,
            0,
            SIBLING_EVENT_TIMEOUT_NS as usize,
        )
    };
    if raw == Errno::EINTR.to_ret() {
        SIBLING_FAIL.store(1, Ordering::Release);
    } else if raw != 0 {
        SIBLING_FAIL.store(2, Ordering::Release);
    }
}

fn sibling_event_waiter(_: usize) {
    SIBLING_ARMED.store(true, Ordering::Release);
    let queue = SIBLING_EVENT_QUEUE.load(Ordering::Acquire);
    let mut output = [EvqEvent::default(); 1];
    // SAFETY: EVQ_WAIT reads one output record from the live stack array and takes the queue
    // handle and scalar timeout; the queue is created by the main thread.
    let raw = unsafe {
        sys::syscall4(
            Syscall::EVQ_WAIT as usize,
            queue as usize,
            output.as_mut_ptr() as usize,
            1,
            SIBLING_EVENT_TIMEOUT_NS as usize,
        )
    };
    if raw == Errno::EINTR.to_ret() {
        SIBLING_FAIL.store(1, Ordering::Release);
    } else if raw != 1 || output[0].cookie != SIBLING_EVENT_COOKIE {
        SIBLING_FAIL.store(2, Ordering::Release);
    }
}

fn sibling_pipe() -> R {
    let (read, write) = pipe().map_err(|_| "pipe")?;
    let stack = core::ptr::addr_of_mut!(SIBLING_STACK);
    let (child, _) = fork_wait_child(sibling_child, child_stack_top(stack)).map_err(|_| "child")?;
    SIBLING_ARMED.store(false, Ordering::Release);
    SIBLING_FAIL.store(0, Ordering::Release);
    let worker =
        thread::spawn(sibling_pipe_waiter, read.0 as usize, STACK_SIZE).map_err(|_| "thread")?;
    sibling_wait_armed()?;
    nanosleep(SIBLING_KICK_NS).map_err(|_| "kick sleep")?;
    let handle = worker.handle().ok_or("no thread handle")?;
    // SAFETY: THREAD_KICK takes the worker thread handle and a scalar bit mask.
    let kicked = unsafe { sys::syscall2(THREAD_KICK, handle.0 as usize, KICK_BIT) };
    if kicked < 0 {
        return Err("kick");
    }
    nanosleep(SIBLING_WAKE_NS).map_err(|_| "wake sleep")?;
    sys::write(write, &[0x5a]).map_err(|_| "write")?;
    worker.join().map_err(|_| "join")?;
    let _ = wait_process(child as u32);
    let _ = handle_close(read);
    let _ = handle_close(write);
    match SIBLING_FAIL.load(Ordering::Acquire) {
        0 => Ok(()),
        1 => Err("eintr"),
        _ => Err("wake"),
    }
}

fn sibling_futex() -> R {
    SIBLING_FUTEX.store(0, Ordering::Release);
    let stack = core::ptr::addr_of_mut!(SIBLING_STACK);
    let (child, _) = fork_wait_child(sibling_child, child_stack_top(stack)).map_err(|_| "child")?;
    SIBLING_ARMED.store(false, Ordering::Release);
    SIBLING_FAIL.store(0, Ordering::Release);
    let worker = thread::spawn(sibling_futex_waiter, 0, STACK_SIZE).map_err(|_| "thread")?;
    sibling_wait_armed()?;
    nanosleep(SIBLING_KICK_NS).map_err(|_| "kick sleep")?;
    let handle = worker.handle().ok_or("no thread handle")?;
    // SAFETY: THREAD_KICK takes the worker thread handle and a scalar bit mask.
    let kicked = unsafe { sys::syscall2(THREAD_KICK, handle.0 as usize, KICK_BIT) };
    if kicked < 0 {
        return Err("kick");
    }
    nanosleep(SIBLING_WAKE_NS).map_err(|_| "wake sleep")?;
    SIBLING_FUTEX.store(1, Ordering::Release);
    let address = core::ptr::addr_of!(SIBLING_FUTEX) as usize;
    // SAFETY: FUTEX WAKE takes the live aligned static address and a scalar wake count.
    let woken =
        unsafe { sys::syscall4(Syscall::FUTEX as usize, address, FUTEX_WAKE as usize, 1, 0) };
    if woken < 0 {
        return Err("futex wake");
    }
    worker.join().map_err(|_| "join")?;
    let _ = wait_process(child as u32);
    match SIBLING_FAIL.load(Ordering::Acquire) {
        0 => Ok(()),
        1 => Err("eintr"),
        _ => Err("wake"),
    }
}

fn sibling_event() -> R {
    let queue = EventQueue::create().map_err(|_| "queue")?;
    let event = event_create().map_err(|_| "event")?;
    queue
        .add(event, SIGNALED, SIBLING_EVENT_COOKIE)
        .map_err(|_| "add")?;
    SIBLING_EVENT_QUEUE.store(queue.handle().0, Ordering::Release);
    let stack = core::ptr::addr_of_mut!(SIBLING_STACK);
    let (child, _) = fork_wait_child(sibling_child, child_stack_top(stack)).map_err(|_| "child")?;
    SIBLING_ARMED.store(false, Ordering::Release);
    SIBLING_FAIL.store(0, Ordering::Release);
    let worker = thread::spawn(sibling_event_waiter, 0, STACK_SIZE).map_err(|_| "thread")?;
    sibling_wait_armed()?;
    nanosleep(SIBLING_KICK_NS).map_err(|_| "kick sleep")?;
    let handle = worker.handle().ok_or("no thread handle")?;
    // SAFETY: THREAD_KICK takes the worker thread handle and a scalar bit mask.
    let kicked = unsafe { sys::syscall2(THREAD_KICK, handle.0 as usize, KICK_BIT) };
    if kicked < 0 {
        return Err("kick");
    }
    nanosleep(SIBLING_WAKE_NS).map_err(|_| "wake sleep")?;
    event_signal(event, 1).map_err(|_| "signal")?;
    worker.join().map_err(|_| "join")?;
    let _ = wait_process(child as u32);
    match SIBLING_FAIL.load(Ordering::Acquire) {
        0 => Ok(()),
        1 => Err("eintr"),
        _ => Err("wake"),
    }
}

fn sibling_exit() -> R {
    let pipe = match sibling_pipe() {
        Ok(()) => "ok",
        Err(error) => error,
    };
    let futex = match sibling_futex() {
        Ok(()) => "ok",
        Err(error) => error,
    };
    let event = match sibling_event() {
        Ok(()) => "ok",
        Err(error) => error,
    };
    println!(
        "readiness: sibling-exit pipe={} futex={} event={}",
        pipe, futex, event
    );
    if pipe == "ok" && futex == "ok" && event == "ok" {
        Ok(())
    } else {
        Err("pipe/futex/event")
    }
}

extern "C" fn sshd_signal_handler(_: usize) -> ! {
    SSHD_HANDLER_RUNS.fetch_add(1, Ordering::AcqRel);
    // SAFETY: the kernel enters this handler with the interrupted EVQ_WAIT context; SIGRETURN
    // restores it after the handler has counted the delivery.
    unsafe { sys::syscall1(SIGRETURN, 0) };
    loop {
        core::hint::spin_loop();
    }
}

extern "C" fn sshd_child(_: usize) -> ! {
    let _ = nanosleep(SSHD_CHILD_NS);
    // SAFETY: EXIT terminates only the forked child process.
    unsafe { sys::syscall1(Syscall::EXIT as usize, SSHD_CHILD_CODE as usize) };
    loop {
        core::hint::spin_loop();
    }
}

fn sshd_waiter(_: usize) {
    let context = core::ptr::addr_of_mut!(SSHD_SIGNAL_CONTEXT) as usize;
    // SAFETY: SIG_SETUP takes scalar user addresses; the context is a live 16-byte-aligned
    // static page and the entry is in the fixture text mapping.
    let setup = unsafe {
        sys::syscall4(
            SIG_SETUP,
            sshd_signal_handler as *const () as usize,
            context,
            SIG_SETUP_ABS as usize,
            0,
        )
    };
    if setup < 0 {
        SSHD_FAIL.store(1, Ordering::Release);
        return;
    }
    let queue = SSHD_QUEUE.load(Ordering::Acquire);
    let stack = core::ptr::addr_of_mut!(SSHD_STACK);
    let (child, pid) = match fork_wait_child(sshd_child, child_stack_top(stack)) {
        Ok(child) => child,
        Err(_) => {
            SSHD_FAIL.store(2, Ordering::Release);
            return;
        }
    };
    SSHD_ARMED.store(true, Ordering::Release);

    let mut output = [EvqEvent::default(); 1];
    let mut got_hup = false;
    for _ in 0..8 {
        // SAFETY: EVQ_WAIT reads one output record from the live stack array and takes the queue
        // handle and scalar timeout; the queue is created by the main thread.
        let raw = unsafe {
            sys::syscall4(
                Syscall::EVQ_WAIT as usize,
                queue as usize,
                output.as_mut_ptr() as usize,
                1,
                SSHD_TIMEOUT_NS as usize,
            )
        };
        if raw == Errno::EINTR.to_ret() {
            SSHD_EINTR.fetch_add(1, Ordering::AcqRel);
            continue;
        }
        if raw == 1 && output[0].events & HUP != 0 {
            got_hup = true;
            break;
        }
        break;
    }
    if !got_hup {
        SSHD_FAIL.store(3, Ordering::Release);
        let _ = handle_close(Handle(child as u32));
        return;
    }

    match wait_one_pid(pid) {
        Ok(status) => {
            SSHD_STATUS.store(status, Ordering::Release);
            if status != SSHD_CHILD_CODE << 8 {
                SSHD_FAIL.store(6, Ordering::Release);
            }
        }
        Err(1) => SSHD_FAIL.store(4, Ordering::Release),
        Err(2) => SSHD_FAIL.store(5, Ordering::Release),
        Err(_) => SSHD_FAIL.store(7, Ordering::Release),
    }
    let _ = handle_close(Handle(child as u32));
}

fn sshd_shape() -> R {
    SSHD_ARMED.store(false, Ordering::Release);
    SSHD_FAIL.store(0, Ordering::Release);
    SSHD_HANDLER_RUNS.store(0, Ordering::Release);
    SSHD_EINTR.store(0, Ordering::Release);
    SSHD_STATUS.store(0, Ordering::Release);

    let queue = EventQueue::create().map_err(|_| "queue")?;
    let (read, write) = pipe().map_err(|_| "pipe")?;
    queue
        .add(read, READABLE | HUP, SSHD_EVENT_COOKIE)
        .map_err(|_| "add pipe")?;
    SSHD_QUEUE.store(queue.handle().0, Ordering::Release);
    let worker = thread::spawn(sshd_waiter, 0, STACK_SIZE).map_err(|_| "thread")?;

    let mut armed = false;
    for _ in 0..PROC_ARM_POLLS {
        if SSHD_ARMED.load(Ordering::Acquire) {
            armed = true;
            break;
        }
        let _ = nanosleep(PROC_ARM_NS);
    }
    if !armed {
        return Err("arm timeout");
    }
    // The child inherited the write end at fork; closing the parent's copy leaves the child as
    // the only writer, so its exit produces the HUP the retry must observe.
    handle_close(write).map_err(|_| "close writer")?;
    worker.join().map_err(|_| "join")?;

    match SSHD_FAIL.load(Ordering::Acquire) {
        0 => {}
        1 => return Err("signal setup"),
        2 => return Err("child fork"),
        3 => return Err("event"),
        4 => return Err("waitpid EINTR"),
        5 => return Err("waitpid wrong pid"),
        6 => return Err("waitpid status"),
        _ => return Err("waitpid"),
    }
    let handler = SSHD_HANDLER_RUNS.load(Ordering::Acquire);
    if handler == 0 {
        return Err("handler");
    }
    println!(
        "readiness: sshd-shape handler={} eintr={} event=hup waitpid={} ok",
        handler,
        SSHD_EINTR.load(Ordering::Acquire),
        SSHD_STATUS.load(Ordering::Acquire)
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
    NET_READY.store(true, Ordering::Release);
    let mut handler = NetHandler;
    server.run(&mut handler);
}

struct FileHandler;

impl Handler for FileHandler {
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

    fn call(&mut self, _file: u64, _request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        let mut encoded = [0u8; 32];
        let length = Response::Ok
            .encode(&mut encoded)
            .map_err(|_| Errno::EPROTO)?;
        let destination = response.get_mut(..length).ok_or(Errno::E2BIG)?;
        destination.copy_from_slice(&encoded[..length]);
        Ok(length)
    }
}

fn file_server(_: usize) {
    let mut server = match Server::register("file") {
        Ok(server) => server,
        Err(error) => {
            println!("readiness: file-server error={}", error.0);
            return;
        }
    };
    FILE_READY.store(true, Ordering::Release);
    let mut handler = FileHandler;
    server.run(&mut handler);
}

fn wait_servers() -> R {
    for _ in 0..1000 {
        if NET_READY.load(Ordering::Acquire) && FILE_READY.load(Ordering::Acquire) {
            return Ok(());
        }
        nanosleep(1_000_000).map_err(|_| "server sleep")?;
    }
    Err("servers")
}

fn net_listen() -> R {
    let image = raven_rt::map_initfs().map_err(|_| "net initfs")?;
    let initfs = Initfs::parse(image).map_err(|_| "net initfs parse")?;
    // The before child is forked while net/file are absent; its namespace snapshot is the
    // pre-registration one. It retries ENOENT until both servers register, then must get EACCES.
    let before = spawn_process(
        &initfs,
        "/child",
        &SpawnOptions {
            args: &["before"],
            env: &[],
        },
    )
    .map_err(|_| "namespace before spawn")?;
    let net = thread::spawn(net_server, 0, STACK_SIZE).map_err(|_| "net server")?;
    let file = thread::spawn(file_server, 0, STACK_SIZE).map_err(|_| "file server")?;
    wait_servers()?;
    let before_code = wait_process(before.0).map_err(|_| "namespace before wait")?;
    if before_code != 0 {
        return Err("strict before");
    }
    // A new child forked after registration gets the live names in its snapshot and must resolve
    // both schemes. This is the positive half of the proof.
    let after = spawn_process(
        &initfs,
        "/child",
        &SpawnOptions {
            args: &["after"],
            env: &[],
        },
    )
    .map_err(|_| "namespace after spawn")?;
    let after_code = wait_process(after.0).map_err(|_| "namespace after wait")?;
    core::mem::forget(net);
    core::mem::forget(file);
    if after_code != 0 {
        return Err("strict after");
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
        ("sibling-exit", sibling_exit),
        ("proc-wait", proc_wait),
        ("sshd-shape", sshd_shape),
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
