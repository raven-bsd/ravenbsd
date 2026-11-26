// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S178 thread and futex conformance probe (test-only, Rust).
//!
//! Runs the POSIX/FreeBSD thread kernel contract on a user-SMP image:
//! futex wait/wake exact returns, value-changed `EAGAIN`, the wake-count limit, requeue moving a
//! waiter, robust-list owner death (`OWNER_DIED` plus one wake), per-thread kernel TLS, thread
//! exit without a kernel destructor and one tid release, and `sched_getaffinity` against the
//! configured boot CPU count.
//!
//! The donor contract is FreeBSD `sys/kern/kern_umtx.c` (`do_wait`, `do_wake`, `do_requeue`,
//! `umtx_handle_rb_event`), `sys/kern/kern_thread.c` (`thread_exit`), `sys/amd64/amd64/sys_machdep.c`
//! (`cpu_set_user_tls`) and `sys/kern/kern_cpuset.c` (`cpuset_which`) for affinity.  No donor
//! expression is copied; the rows assert the observable contract.
//!
//! Two controlled mutations encode what a broken implementation would believe and therefore turn
//! their rows red against the correct product:
//!   wake-one-extra   -- `FUTEX_WAKE(1)` is expected to wake two waiters
//!   tls-shared-base  -- two threads are expected to share one TLS base

#![no_std]
#![no_main]

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::fmt;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "threads";

/// Raw syscall numbers used by the probe (`crates/raven-abi/src/syscall.rs`).
const SYS_YIELD: usize = 0x02;
const SYS_THREAD_CREATE: usize = 0x20;
const SYS_THREAD_EXIT: usize = 0x21;
const SYS_FUTEX: usize = 0x22;
const SYS_THREAD_SELF: usize = 0x24;
const SYS_THREAD_EXIT_EX: usize = 0x25;
const SYS_ROBUST_LIST_SET: usize = 0x26;
/// `THREAD_CREATE` flag carrying a [`ThreadCreateEx`] payload.
const THREAD_CREATE_EX: usize = 1;
/// `SELF_PROC`: the calling process.
const SELF_PROC: usize = 0;

/// `futex` operations, Linux numbering (`raven-abi` names WAIT/WAKE; REQUEUE is the PG-3 seam).
const FUTEX_WAIT: u32 = 0;
const FUTEX_WAKE: u32 = 1;
const FUTEX_REQUEUE: u32 = 3;
const FUTEX_PRIVATE: u32 = 0x80;
/// Robust futex word bits.
const OWNER_DIED: u32 = 0x4000_0000;
const OWNER_TID: u32 = 0x3fff_ffff;

const EAGAIN: i32 = 11;
const ETIMEDOUT: i32 = 110;

const WAIT_TIMEOUT_NS: u64 = 2_000_000_000;
const LIMIT_TIMEOUT_NS: u64 = 400_000_000;
const ROBUST_TIMEOUT_NS: u64 = 2_000_000_000;
/// Upper bound on a scheduling settle: enough for a waiter on another CPU to enqueue.
const SETTLE_NS: u64 = 100_000_000;
/// Bound on a yield loop waiting for a worker to arm.
const SPIN_LIMIT: usize = 5_000_000;
const TLS_ROUNDS: usize = 1000;

const CLOCK_MONOTONIC: c_int = 1;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

/// `THREAD_CREATE` extended payload (`raven-abi` `ThreadCreateEx`).
#[repr(C)]
#[derive(Clone, Copy)]
struct ThreadCreateEx {
    tp: usize,
    tid_out: usize,
    clear_tid_addr: usize,
}

const _: () = assert!(core::mem::size_of::<ThreadCreateEx>() == 24);

/// Linux `struct robust_list_head` (24 bytes on the 64-bit ABI).
#[repr(C, align(8))]
struct RobustHead {
    next: u64,
    offset: i64,
    pending: u64,
}

unsafe extern "C" {
    fn pthread_create(
        thread: *mut usize,
        attr: *const c_void,
        start: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> c_int;
    fn pthread_join(thread: usize, value: *mut *mut c_void) -> c_int;
    fn sched_getaffinity(pid: c_int, size: usize, mask: *mut u8) -> c_int;
    fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
}

// --- raw kernel entry -------------------------------------------------------------------------

/// Raw syscall with no arguments.
///
/// # Safety
///
/// `number` must be a RavenBSD syscall that takes no pointer arguments.
unsafe fn syscall0(number: usize) -> isize {
    let result: isize;
    // SAFETY: `syscall` follows the raven-abi convention (number in `rax`, result in `rax`).
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number as isize => result,
            clobber_abi("C"),
            options(nostack),
        );
    }
    result
}

/// Raw syscall with one argument.
///
/// # Safety
///
/// `number` must be a RavenBSD syscall taking `a0`.
unsafe fn syscall1(number: usize, a0: usize) -> isize {
    let result: isize;
    // SAFETY: `syscall` follows the raven-abi convention (argument in `rdi`, result in `rax`).
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number as isize => result,
            in("rdi") a0,
            clobber_abi("C"),
            options(nostack),
        );
    }
    result
}

/// Raw syscall with two arguments.
///
/// # Safety
///
/// `number` must be a RavenBSD syscall taking `a0..a1`; pointer arguments must be valid.
unsafe fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    let result: isize;
    // SAFETY: `syscall` follows the raven-abi convention (arguments in `rdi`, `rsi`; result in
    // `rax`).
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number as isize => result,
            in("rdi") a0,
            in("rsi") a1,
            clobber_abi("C"),
            options(nostack),
        );
    }
    result
}

/// Raw syscall with four arguments.
///
/// # Safety
///
/// `number` must be a RavenBSD syscall taking `a0..a3`; pointer arguments must be valid.
unsafe fn syscall4(number: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let result: isize;
    // SAFETY: `syscall` follows the raven-abi convention (arguments in `rdi`, `rsi`, `rdx`,
    // `r10`; result in `rax`).
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number as isize => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            clobber_abi("C"),
            options(nostack),
        );
    }
    result
}

/// Raw syscall with six arguments.
///
/// # Safety
///
/// `number` must be a RavenBSD syscall taking `a0..a5`; pointer arguments must be valid.
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: `syscall` follows the raven-abi convention (arguments in `rdi`, `rsi`, `rdx`,
    // `r10`, `r8`, `r9`; result in `rax`).
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") number as isize => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            clobber_abi("C"),
            options(nostack),
        );
    }
    result
}

fn futex(word: &AtomicU32, op: u32, value: u32, timeout_ns: u64) -> isize {
    // SAFETY: `word` is a live four-byte aligned user word for the duration of the call.
    unsafe {
        syscall4(
            SYS_FUTEX,
            word.as_ptr() as usize,
            (op | FUTEX_PRIVATE) as usize,
            value as usize,
            timeout_ns as usize,
        )
    }
}

fn yield_now() {
    // SAFETY: YIELD takes no arguments and has no side effect beyond scheduling.
    unsafe {
        syscall0(SYS_YIELD);
    }
}

fn now_ns() -> u64 {
    let mut time = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a live writable `timespec`.
    if unsafe { clock_gettime(CLOCK_MONOTONIC, &mut time) } != 0 {
        return 0;
    }
    (time.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(time.tv_nsec.max(0) as u64)
}

/// The calling thread's kernel TLS base (`%fs:0` is the x86-64 self pointer).
fn thread_pointer() -> usize {
    let pointer: usize;
    // SAFETY: reading `fs:0` is the x86-64 self-pointer convention and is side-effect free.
    unsafe {
        core::arch::asm!(
            "mov {}, qword ptr fs:[0]",
            out(reg) pointer,
            options(nostack, nomem, preserves_flags),
        );
    }
    pointer
}

fn spin_until(condition: impl Fn() -> bool) -> bool {
    for _ in 0..SPIN_LIMIT {
        if condition() {
            return true;
        }
        yield_now();
    }
    false
}

fn settle() {
    let start = now_ns();
    while now_ns().saturating_sub(start) < SETTLE_NS {
        yield_now();
    }
}

// --- pthread helpers --------------------------------------------------------------------------

fn spawn(start: extern "C" fn(*mut c_void) -> *mut c_void, arg: usize) -> Option<usize> {
    let mut thread = 0usize;
    // SAFETY: a default (joinable) attribute and a live start routine; `arg` is a scalar index.
    let status =
        unsafe { pthread_create(&mut thread, core::ptr::null(), start, arg as *mut c_void) };
    if status == 0 { Some(thread) } else { None }
}

fn join(thread: usize) {
    // SAFETY: `thread` is a joinable pthread created by `spawn`; the return value is unused.
    unsafe {
        pthread_join(thread, null_mut());
    }
}

// --- futex wait/wake rows ---------------------------------------------------------------------

static WAIT_WORD: AtomicU32 = AtomicU32::new(0);
static WAIT_ARMED: AtomicU32 = AtomicU32::new(0);
static WAIT_RC: AtomicI32 = AtomicI32::new(i32::MIN);

extern "C" fn wait_worker(_arg: *mut c_void) -> *mut c_void {
    WAIT_ARMED.store(1, Ordering::SeqCst);
    let result = futex(&WAIT_WORD, FUTEX_WAIT, 0, WAIT_TIMEOUT_NS);
    WAIT_RC.store(result as i32, Ordering::SeqCst);
    null_mut()
}

/// Wake until one call returns a nonzero count; the first nonzero value is the kernel's answer.
fn first_wake(word: &AtomicU32, count: u32) -> Option<usize> {
    for _ in 0..SPIN_LIMIT {
        let result = futex(word, FUTEX_WAKE, count, 0);
        if result < 0 {
            return None;
        }
        if result > 0 {
            return Some(result as usize);
        }
        yield_now();
    }
    None
}

fn row_futex_wait_wake(state: &mut State) {
    WAIT_WORD.store(0, Ordering::SeqCst);
    WAIT_ARMED.store(0, Ordering::SeqCst);
    WAIT_RC.store(i32::MIN, Ordering::SeqCst);
    let Some(thread) = spawn(wait_worker, 0) else {
        state.check("futex-wait-wake", false, format_args!("spawn=failed"));
        return;
    };
    let armed = spin_until(|| WAIT_ARMED.load(Ordering::SeqCst) == 1);
    let woken = first_wake(&WAIT_WORD, 1);
    join(thread);
    let wait_rc = WAIT_RC.load(Ordering::SeqCst);
    let pass = armed && woken == Some(1) && wait_rc == 0;
    state.check(
        "futex-wait-wake",
        pass,
        format_args!(
            "armed={} wake_rc={} wait_rc={}",
            flag(armed),
            number(woken),
            wait_rc
        ),
    );
}

fn row_futex_eagain(state: &mut State) {
    let word = AtomicU32::new(0x1234);
    let result = futex(&word, FUTEX_WAIT, 0x1235, 0);
    let pass = result == -(EAGAIN as isize);
    state.check(
        "futex-eagain",
        pass,
        format_args!("word=4660 expected=4661 rc={}", result),
    );
}

static LIMIT_WORD: AtomicU32 = AtomicU32::new(0);
static LIMIT_ARMED: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
static LIMIT_RC: [AtomicI32; 3] = [
    AtomicI32::new(i32::MIN),
    AtomicI32::new(i32::MIN),
    AtomicI32::new(i32::MIN),
];

extern "C" fn limit_worker(arg: *mut c_void) -> *mut c_void {
    let index = arg as usize;
    LIMIT_ARMED[index].store(1, Ordering::SeqCst);
    let result = futex(&LIMIT_WORD, FUTEX_WAIT, 0, LIMIT_TIMEOUT_NS);
    LIMIT_RC[index].store(result as i32, Ordering::SeqCst);
    null_mut()
}

fn run_wake_limit(state: &mut State, mutation: bool) {
    LIMIT_WORD.store(0, Ordering::SeqCst);
    for index in 0..3 {
        LIMIT_ARMED[index].store(0, Ordering::SeqCst);
        LIMIT_RC[index].store(i32::MIN, Ordering::SeqCst);
    }
    let mut threads = [0usize; 3];
    for index in 0..3 {
        match spawn(limit_worker, index) {
            Some(thread) => threads[index] = thread,
            None => {
                state.check(
                    "futex-wake-limit",
                    false,
                    format_args!("spawn{index}=failed"),
                );
                return;
            }
        }
    }
    let armed = spin_until(|| {
        LIMIT_ARMED
            .iter()
            .all(|slot| slot.load(Ordering::SeqCst) == 1)
    });
    let first = first_wake(&LIMIT_WORD, 1);
    for thread in threads {
        join(thread);
    }
    let returned = LIMIT_RC
        .iter()
        .filter(|slot| slot.load(Ordering::SeqCst) == 0)
        .count();
    let timed_out = LIMIT_RC
        .iter()
        .filter(|slot| slot.load(Ordering::SeqCst) == -(ETIMEDOUT as i32))
        .count();
    // The mutation believes a count-1 wake wakes two waiters; the correct product wakes one.
    let expected_wake = if mutation { 2 } else { 1 };
    let expected_returned = if mutation { 2 } else { 1 };
    let after = futex(&LIMIT_WORD, FUTEX_WAKE, 1, 0);
    let pass = armed
        && first == Some(expected_wake)
        && returned == expected_returned
        && timed_out == 3 - expected_returned
        && after == 0;
    state.check(
        "futex-wake-limit",
        pass,
        format_args!(
            "armed={} first_wake={} expect_wake={} returned={} timed_out={} after={} mutation={}",
            flag(armed),
            number(first),
            expected_wake,
            returned,
            timed_out,
            after,
            flag(mutation)
        ),
    );
}

fn row_futex_wake_limit(state: &mut State) {
    run_wake_limit(state, false);
}

// --- requeue ----------------------------------------------------------------------------------

static REQ_SOURCE: AtomicU32 = AtomicU32::new(0);
static REQ_TARGET: AtomicU32 = AtomicU32::new(0);
static REQ_ARMED: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static REQ_RC: [AtomicI32; 2] = [AtomicI32::new(i32::MIN), AtomicI32::new(i32::MIN)];

extern "C" fn requeue_worker(arg: *mut c_void) -> *mut c_void {
    let index = arg as usize;
    REQ_ARMED[index].store(1, Ordering::SeqCst);
    let result = futex(&REQ_SOURCE, FUTEX_WAIT, 0, LIMIT_TIMEOUT_NS);
    REQ_RC[index].store(result as i32, Ordering::SeqCst);
    null_mut()
}

fn row_futex_requeue(state: &mut State) {
    REQ_SOURCE.store(0, Ordering::SeqCst);
    REQ_TARGET.store(0, Ordering::SeqCst);
    for index in 0..2 {
        REQ_ARMED[index].store(0, Ordering::SeqCst);
        REQ_RC[index].store(i32::MIN, Ordering::SeqCst);
    }
    let mut threads = [0usize; 2];
    for index in 0..2 {
        match spawn(requeue_worker, index) {
            Some(thread) => threads[index] = thread,
            None => {
                state.check("futex-requeue", false, format_args!("spawn{index}=failed"));
                return;
            }
        }
    }
    let armed = spin_until(|| {
        REQ_ARMED
            .iter()
            .all(|slot| slot.load(Ordering::SeqCst) == 1)
    });
    // The native `futex(addr, op, val, timeout_ns)` contract has no argument slot for the
    // destination address or the requeue maximum; a requeue request is the best the ABI permits.
    let requeue = futex(&REQ_SOURCE, FUTEX_REQUEUE, 1, 0);
    // Wake both queues directly: a correct requeue leaves the source empty and the destination
    // holding both waiters, so these two counts separate "requeue missing" from "wait/wake
    // broken".
    let wake_source = futex(&REQ_SOURCE, FUTEX_WAKE, 2, 0);
    let wake_target = futex(&REQ_TARGET, FUTEX_WAKE, 2, 0);
    for thread in threads {
        join(thread);
    }
    let released = REQ_RC
        .iter()
        .filter(|slot| slot.load(Ordering::SeqCst) == 0)
        .count();
    let pass = requeue == 0 && wake_source == 0 && wake_target == 2 && released == 2;
    state.check(
        "futex-requeue",
        pass,
        format_args!(
            "armed={} requeue_rc={} expected=0 wake_source={} wake_target={} released={}",
            flag(armed),
            requeue,
            wake_source,
            wake_target,
            released
        ),
    );
}

// --- robust owner death -----------------------------------------------------------------------

struct RobustState {
    head: RobustHead,
    node: u64,
    word: AtomicU32,
}

/// A single-writer shared cell: the probe uses it for state published before a worker starts.
struct Shared<T>(UnsafeCell<T>);

// SAFETY: every use in this probe is sequenced (owner setup before spawn, kernel word after
// those writes); the raw threads only read the published word.
unsafe impl<T> Sync for Shared<T> {}

static ROBUST: Shared<RobustState> = Shared(UnsafeCell::new(RobustState {
    head: RobustHead {
        next: 0,
        offset: 0,
        pending: 0,
    },
    node: 0,
    word: AtomicU32::new(0),
}));

fn robust_state() -> &'static mut RobustState {
    // SAFETY: the caller is the single setup writer or the owner thread before the kernel walk.
    unsafe { &mut *ROBUST.0.get() }
}
static ROBUST_OWNER_ARMED: AtomicU32 = AtomicU32::new(0);
static ROBUST_WAITER_ARMED: AtomicU32 = AtomicU32::new(0);
static ROBUST_WAIT_RC: AtomicI32 = AtomicI32::new(i32::MIN);

fn robust_word() -> &'static AtomicU32 {
    // SAFETY: the structure is static and the reference only ever points at the stable word.
    unsafe { &(*ROBUST.0.get()).word }
}

extern "C" fn robust_owner(_arg: *mut c_void) -> *mut c_void {
    // SAFETY: this is the only writer of the structure before the kernel walk.
    let state = robust_state();
    // SAFETY: THREAD_SELF takes no arguments.
    let tid = unsafe { syscall0(SYS_THREAD_SELF) } as u32;
    state.word.store(tid & OWNER_TID, Ordering::SeqCst);
    let head = core::ptr::addr_of_mut!(state.head) as u64;
    let node = core::ptr::addr_of_mut!(state.node) as u64;
    let word = state.word.as_ptr() as i64;
    state.head.next = node;
    state.head.offset = word - node as i64;
    state.head.pending = 0;
    let mut node_value = head;
    core::mem::swap(&mut state.node, &mut node_value);
    // SAFETY: `head` is a live 24-byte user `robust_list_head`.
    let registered = unsafe { syscall2(SYS_ROBUST_LIST_SET, head as usize, 24) };
    // 1 = registered; 2 = the kernel rejected the list.  Main waits for 1, never for a raw
    // return value (0 is success).
    ROBUST_OWNER_ARMED.store(if registered == 0 { 1 } else { 2 }, Ordering::SeqCst);
    if registered != 0 {
        // SAFETY: THREAD_EXIT never returns.
        unsafe {
            syscall1(SYS_THREAD_EXIT, 1);
        }
    }
    // Let the waiter reach FUTEX_WAIT on another CPU before the exit walks the robust list.
    let _ = spin_until(|| ROBUST_WAITER_ARMED.load(Ordering::SeqCst) == 1);
    settle();
    // SAFETY: THREAD_EXIT_EX(code, clear_addr=0, unmap_addr=0, unmap_len=0) walks the robust
    // list, then terminates this thread and never returns.
    unsafe {
        syscall4(SYS_THREAD_EXIT_EX, 0, 0, 0, 0);
    }
    null_mut()
}

extern "C" fn robust_waiter(_arg: *mut c_void) -> *mut c_void {
    let word = robust_word();
    let owner = word.load(Ordering::SeqCst);
    ROBUST_WAITER_ARMED.store(1, Ordering::SeqCst);
    let result = futex(word, FUTEX_WAIT, owner, ROBUST_TIMEOUT_NS);
    ROBUST_WAIT_RC.store(result as i32, Ordering::SeqCst);
    null_mut()
}

fn row_robust_owner_died(state: &mut State) {
    ROBUST_OWNER_ARMED.store(0, Ordering::SeqCst);
    ROBUST_WAITER_ARMED.store(0, Ordering::SeqCst);
    ROBUST_WAIT_RC.store(i32::MIN, Ordering::SeqCst);
    {
        // SAFETY: no worker runs yet; this is the only writer.
        let state = robust_state();
        state.word.store(0, Ordering::SeqCst);
        state.head.next = 0;
        state.head.offset = 0;
        state.head.pending = 0;
        state.node = 0;
    }
    let Some(owner) = spawn(robust_owner, 0) else {
        state.check(
            "robust-owner-died",
            false,
            format_args!("owner_spawn=failed"),
        );
        return;
    };
    let owner_armed = spin_until(|| ROBUST_OWNER_ARMED.load(Ordering::SeqCst) != 0);
    let Some(waiter) = spawn(robust_waiter, 0) else {
        state.check(
            "robust-owner-died",
            false,
            format_args!("waiter_spawn=failed"),
        );
        return;
    };
    let waited = spin_until(|| ROBUST_WAIT_RC.load(Ordering::SeqCst) != i32::MIN);
    join(waiter);
    // The owner never returns through libc; it leaves through THREAD_EXIT_EX, so it is not
    // joined.  Its pthread control block is process-lifetime state in this one-shot probe.
    let _ = owner;
    let wait_rc = ROBUST_WAIT_RC.load(Ordering::SeqCst);
    let word = robust_word().load(Ordering::SeqCst);
    let extra = futex(robust_word(), FUTEX_WAKE, 1, 0);
    let pass = owner_armed && waited && wait_rc == 0 && word == OWNER_DIED && extra == 0;
    state.check(
        "robust-owner-died",
        pass,
        format_args!(
            "owner_armed={} waited={} wait_rc={} word={:#x} expected={:#x} extra_wake={}",
            flag(owner_armed),
            flag(waited),
            wait_rc,
            word,
            OWNER_DIED,
            extra
        ),
    );
}

// --- TLS --------------------------------------------------------------------------------------

static TLS_TP: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];
static TLS_STABLE: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];

extern "C" fn tls_worker(arg: *mut c_void) -> *mut c_void {
    let index = arg as usize;
    let base = thread_pointer();
    TLS_TP[index].store(base, Ordering::SeqCst);
    let mut stable = base != 0;
    for _ in 0..TLS_ROUNDS {
        if thread_pointer() != base {
            stable = false;
            break;
        }
        yield_now();
    }
    TLS_STABLE[index].store(u32::from(stable), Ordering::SeqCst);
    null_mut()
}

fn run_tls(state: &mut State, mutation: bool) {
    for index in 0..2 {
        TLS_TP[index].store(0, Ordering::SeqCst);
        TLS_STABLE[index].store(0, Ordering::SeqCst);
    }
    let mut threads = [0usize; 2];
    for index in 0..2 {
        match spawn(tls_worker, index) {
            Some(thread) => threads[index] = thread,
            None => {
                state.check("tls-per-thread", false, format_args!("spawn{index}=failed"));
                return;
            }
        }
    }
    let main_base = thread_pointer();
    for thread in threads {
        join(thread);
    }
    let first = TLS_TP[0].load(Ordering::SeqCst);
    let second = TLS_TP[1].load(Ordering::SeqCst);
    let stable =
        TLS_STABLE[0].load(Ordering::SeqCst) == 1 && TLS_STABLE[1].load(Ordering::SeqCst) == 1;
    // The mutation believes the two threads share one TLS base; the correct product keeps them
    // distinct and each stable across yields and preemption.
    let distinct =
        first != 0 && second != 0 && first != second && first != main_base && second != main_base;
    let pass = stable && distinct != mutation;
    state.check(
        "tls-per-thread",
        pass,
        format_args!(
            "main={:#x} first={:#x} second={:#x} distinct={} stable={} mutation={}",
            main_base,
            first,
            second,
            flag(distinct),
            flag(stable),
            flag(mutation)
        ),
    );
}

fn row_tls_per_thread(state: &mut State) {
    run_tls(state, false);
}

// --- thread exit and tid release --------------------------------------------------------------

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; 64 * 1024]);

static mut RAW_STACKS: [Stack; 2] = [const { Stack([0; 64 * 1024]) }; 2];
static mut TLS_BLOCKS: [[u64; 8]; 2] = [[0; 8]; 2];
static RAW_RAN: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static RAW_TLS_MATCH: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static RAW_TID_OUT: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static RAW_CLEAR: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
/// Sentinel stored in the clear-on-exit word before the thread runs.
const CLEAR_SENTINEL: u32 = 0xdead_beef;

fn raw_stack_top(index: usize) -> usize {
    let base = core::ptr::addr_of_mut!(RAW_STACKS) as *mut Stack;
    // SAFETY: `index < 2`, the array is static, and only the address is used as a stack pointer.
    let slot = unsafe { base.add(index) };
    (slot as usize + core::mem::size_of::<Stack>()) & !0xf
}

fn tls_block(index: usize) -> *mut u64 {
    let base = core::ptr::addr_of_mut!(TLS_BLOCKS) as *mut [u64; 8];
    // SAFETY: `index < 2` and the array is static.
    unsafe { base.add(index) as *mut u64 }
}

fn tid_out(index: usize) -> *mut u32 {
    RAW_TID_OUT[index].as_ptr()
}

fn clear_word(index: usize) -> *mut u32 {
    RAW_CLEAR[index].as_ptr()
}

extern "C" fn raw_exit_worker(arg: *mut c_void) -> *mut c_void {
    let index = arg as usize;
    let expected = tls_block(index) as usize;
    RAW_TLS_MATCH[index].store(u32::from(thread_pointer() == expected), Ordering::SeqCst);
    RAW_RAN[index].store(1, Ordering::SeqCst);
    // SAFETY: THREAD_EXIT never returns; it walks the (empty) robust list and writes the
    // clear-on-exit word exactly once.
    unsafe {
        syscall1(SYS_THREAD_EXIT, 0);
    }
    null_mut()
}

/// Create one raw thread with an extended payload; returns its kernel tid when the kernel wrote it.
fn create_raw(index: usize) -> Option<u32> {
    let block = tls_block(index);
    // The x86-64 self pointer is the first word of the thread control block.
    // SAFETY: the block is static, aligned and not yet used by another thread.
    unsafe {
        *block = block as usize as u64;
    }
    RAW_RAN[index].store(0, Ordering::SeqCst);
    RAW_TLS_MATCH[index].store(0, Ordering::SeqCst);
    RAW_TID_OUT[index].store(0, Ordering::SeqCst);
    RAW_CLEAR[index].store(CLEAR_SENTINEL, Ordering::SeqCst);
    let extended = ThreadCreateEx {
        tp: block as usize,
        tid_out: tid_out(index) as usize,
        clear_tid_addr: clear_word(index) as usize,
    };
    // SAFETY: the entry, stack top and payload are live user addresses for the call; the kernel
    // validates them.
    let handle = unsafe {
        syscall6(
            SYS_THREAD_CREATE,
            SELF_PROC,
            raw_exit_worker as *const () as usize,
            raw_stack_top(index),
            index,
            THREAD_CREATE_EX,
            &extended as *const ThreadCreateEx as usize,
        )
    };
    if handle < 0 {
        return None;
    }
    if !spin_until(|| RAW_CLEAR[index].load(Ordering::SeqCst) == 0) {
        return None;
    }
    Some(RAW_TID_OUT[index].load(Ordering::SeqCst))
}

fn row_thread_exit_tid(state: &mut State) {
    let first = create_raw(0);
    let second = create_raw(1);
    let ran = RAW_RAN[0].load(Ordering::SeqCst) == 1 && RAW_RAN[1].load(Ordering::SeqCst) == 1;
    let tls_match = RAW_TLS_MATCH[0].load(Ordering::SeqCst) == 1
        && RAW_TLS_MATCH[1].load(Ordering::SeqCst) == 1;
    let cleared =
        RAW_CLEAR[0].load(Ordering::SeqCst) == 0 && RAW_CLEAR[1].load(Ordering::SeqCst) == 0;
    let tids = match (first, second) {
        (Some(first), Some(second)) => Some((first, second)),
        _ => None,
    };
    let released_once =
        tids.is_some_and(|(first, second)| first != 0 && second != 0 && first != second);
    // The kernel runs no destructor: the thread's TLS self pointer is untouched after exit.
    // SAFETY: both blocks are static and the raw threads have terminated.
    let tls_intact = unsafe {
        *tls_block(0) == tls_block(0) as usize as u64
            && *tls_block(1) == tls_block(1) as usize as u64
    };
    let pass = ran && tls_match && cleared && released_once && tls_intact;
    state.check(
        "thread-exit-tid",
        pass,
        format_args!(
            "ran={} tls_match={} cleared={} tid1={} tid2={} unique={} tls_intact={}",
            flag(ran),
            flag(tls_match),
            flag(cleared),
            number_u32(first),
            number_u32(second),
            flag(released_once),
            flag(tls_intact)
        ),
    );
}

// --- affinity ---------------------------------------------------------------------------------

fn row_sched_getaffinity(state: &mut State, expected_cpus: usize) {
    let mut mask = [0u8; 16];
    // pid 0 is the calling process; the mask must have room for the machine's CPUs.
    // SAFETY: `mask` is a live writable 16-byte `cpu_set_t`.
    let result = unsafe { sched_getaffinity(0, mask.len(), mask.as_mut_ptr()) };
    let reported: usize = mask.iter().map(|byte| byte.count_ones() as usize).sum();
    let pass = result == 0 && reported == expected_cpus;
    state.check(
        "sched-getaffinity",
        pass,
        format_args!(
            "rc={} reported={} expected={} mask={:02x}{:02x}{:02x}{:02x}",
            result, reported, expected_cpus, mask[0], mask[1], mask[2], mask[3]
        ),
    );
}

// --- reporting helpers ------------------------------------------------------------------------

fn flag(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn number(value: Option<usize>) -> Number {
    Number(value)
}

fn number_u32(value: Option<u32>) -> Number {
    Number(value.map(|inner| inner as usize))
}

struct Number(Option<usize>);

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(formatter, "{value}"),
            None => formatter.write_str("none"),
        }
    }
}

// --- entry ------------------------------------------------------------------------------------

fn decode_u32(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}

fn c_string_eq(pointer: *const u8, expected: &[u8]) -> bool {
    if pointer.is_null() {
        return false;
    }
    for (index, &byte) in expected.iter().enumerate() {
        // SAFETY: argv strings are NUL-terminated and the caller compares within that bound.
        if unsafe { *pointer.add(index) } != byte {
            return false;
        }
    }
    // SAFETY: the expected text is a prefix; require the NUL terminator next.
    unsafe { *pointer.add(expected.len()) == 0 }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    conformance::emit(format_args!("CONFORM {TOPIC} panic\n"));
    // SAFETY: THREAD_EXIT terminates the process's last thread with a failure status.
    unsafe {
        syscall1(SYS_THREAD_EXIT, 2);
    }
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const u8) -> c_int {
    let mut run_all = false;
    let mut cpus = 1usize;
    let mut mutation: Option<*const u8> = None;
    for index in 1..argc as usize {
        // SAFETY: index is inside argv.
        let argument = unsafe { *argv.add(index) };
        if c_string_eq(argument, b"--all") {
            run_all = true;
        } else if c_string_eq(argument, b"--cpus") && index + 1 < argc as usize {
            // SAFETY: the next entry is inside argv.
            let value = unsafe { *argv.add(index + 1) };
            let bytes = {
                let mut length = 0usize;
                // SAFETY: argv strings are NUL-terminated.
                while length < 8 && unsafe { *value.add(length) } != 0 {
                    length += 1;
                }
                // SAFETY: `length` bytes were just checked readable.
                unsafe { core::slice::from_raw_parts(value, length) }
            };
            if let Some(parsed) = decode_u32(bytes) {
                cpus = parsed as usize;
            }
        } else if c_string_eq(argument, b"--mutation") && index + 1 < argc as usize {
            // SAFETY: the next entry is inside argv.
            mutation = Some(unsafe { *argv.add(index + 1) });
        }
    }

    let mut state = State::begin(TOPIC);
    if run_all {
        row_futex_wait_wake(&mut state);
        row_futex_eagain(&mut state);
        row_futex_wake_limit(&mut state);
        row_futex_requeue(&mut state);
        row_robust_owner_died(&mut state);
        row_tls_per_thread(&mut state);
        row_thread_exit_tid(&mut state);
        row_sched_getaffinity(&mut state, cpus);

        // Controlled mutations: each encodes a broken implementation's belief, so the correct
        // product turns its targeted row red.
        let wake_one_extra = mutation.is_some_and(|name| c_string_eq(name, b"wake-one-extra"));
        let tls_shared_base = mutation.is_some_and(|name| c_string_eq(name, b"tls-shared-base"));
        if wake_one_extra || mutation.is_none() {
            conformance::mutation_begin(TOPIC, "wake-one-extra");
            let mut section = State::begin(TOPIC);
            run_wake_limit(&mut section, true);
            let _ = conformance::mutation_end(&section, "wake-one-extra");
        }
        if tls_shared_base || mutation.is_none() {
            conformance::mutation_begin(TOPIC, "tls-shared-base");
            let mut section = State::begin(TOPIC);
            run_tls(&mut section, true);
            let _ = conformance::mutation_end(&section, "tls-shared-base");
        }
    } else {
        state.check("selection", false, format_args!("no --all"));
    }
    state.finish()
}
