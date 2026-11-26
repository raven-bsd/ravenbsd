// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S176 clock and sleep conformance probe (test-only, Rust).
//!
//! Runs the POSIX/FreeBSD clock contract on the release image: `CLOCK_REALTIME`/`CLOCK_MONOTONIC`
//! ids and rates, `clock_getres`, a monotonic clock that never goes backwards over 10,000 samples
//! (single-thread and two-thread SMP), `nanosleep` sleeping at least the request, `EINTR` with the
//! positive unslept remainder after a handled signal, and `gettimeofday` agreeing with
//! `CLOCK_REALTIME`.
//!
//! The donor contract is FreeBSD `sys/kern/kern_time.c` (`kern_clock_gettime`,
//! `kern_clock_getres`, `kern_gettimeofday`, `kern_nanosleep`) and `sys/kern/kern_clock.c`
//! (`hardclock`, `cpu_tick`), NetBSD `sys/kern/kern_time.c` (`clock_gettime1`, `gettimeofday1`,
//! `nanosleep1`), and the POSIX clock/nanosleep definitions. No donor expression is copied; the
//! rows assert the observable contract.
//!
//! The controlled mutation encodes what a broken implementation would believe and therefore turns
//! its row red against the correct product:
//!   early-sleep -- a 1 ms sleep is expected to satisfy the 100 ms at-least assertion

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_void};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicI32, Ordering};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "clocks";

const CLOCK_REALTIME: c_int = 0;
const CLOCK_MONOTONIC: c_int = 1;

const EINTR: c_int = 4;

const SIGUSR1: c_int = 10;

const NANOS_PER_SECOND: i64 = 1_000_000_000;
const SLEEP_NS: i64 = 100_000_000;
const SAMPLES: i32 = 10_000;

static SIGNAL_SEEN: AtomicI32 = AtomicI32::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

impl Timespec {
    const ZERO: Self = Self {
        tv_sec: 0,
        tv_nsec: 0,
    };
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Timeval {
    tv_sec: i64,
    tv_usec: i64,
}

impl Timeval {
    const ZERO: Self = Self {
        tv_sec: 0,
        tv_usec: 0,
    };
}

/// The public 128-byte `sigset_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigset {
    bytes: [u64; 16],
}

impl Sigset {
    const ZERO: Self = Self { bytes: [0; 16] };
}

/// The public 152-byte C `struct sigaction`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigaction {
    sa_handler: usize,
    sa_mask: Sigset,
    sa_flags: c_int,
    _pad: c_int,
    sa_restorer: usize,
}

const _: () = assert!(core::mem::size_of::<Sigaction>() == 152);

unsafe extern "C" {
    fn clock_gettime(id: c_int, value: *mut Timespec) -> c_int;
    fn clock_getres(id: c_int, value: *mut Timespec) -> c_int;
    fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> c_int;
    fn gettimeofday(value: *mut Timeval, zone: *mut c_void) -> c_int;
    fn fork() -> c_int;
    fn getppid() -> c_int;
    fn kill(pid: c_int, signal: c_int) -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn sigaction(signal: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn sigemptyset(set: *mut Sigset) -> c_int;
    fn pthread_create(
        thread: *mut usize,
        attribute: *const c_void,
        start: extern "C" fn(*mut c_void) -> *mut c_void,
        argument: *mut c_void,
    ) -> c_int;
    fn pthread_join(thread: usize, result: *mut *mut c_void) -> c_int;
    fn __errno_location() -> *mut c_int;
    fn _exit(status: c_int) -> !;
}

fn errno() -> c_int {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() }
}

fn set_errno(value: c_int) {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() = value };
}

fn ns_of(value: &Timespec) -> i64 {
    value
        .tv_sec
        .saturating_mul(NANOS_PER_SECOND)
        .saturating_add(value.tv_nsec)
}

fn clock_now(id: c_int) -> Option<i64> {
    let mut value = Timespec::ZERO;
    // SAFETY: `value` is a writable timespec.
    if unsafe { clock_gettime(id, &mut value) } != 0 {
        return None;
    }
    Some(ns_of(&value))
}

fn flag(value: bool) -> i32 {
    if value { 1 } else { 0 }
}

extern "C" fn note_signal(_signal: c_int) {
    SIGNAL_SEEN.fetch_add(1, Ordering::SeqCst);
}

fn install_signal(signal: c_int) -> bool {
    let mut action = Sigaction {
        sa_handler: note_signal as *const () as usize,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: the action and mask are fully initialized.
    unsafe {
        sigemptyset(&mut action.sa_mask);
        sigaction(signal, &action, null_mut()) == 0
    }
}

fn row_ids(state: &mut State) {
    let ok = CLOCK_REALTIME == 0 && CLOCK_MONOTONIC == 1;
    state.check(
        "ids",
        ok,
        format_args!(
            "realtime={} monotonic={} expect_realtime=0 expect_monotonic=1",
            CLOCK_REALTIME, CLOCK_MONOTONIC
        ),
    );
}

fn row_getres(state: &mut State) {
    let mut first = Timespec::ZERO;
    let mut second = Timespec::ZERO;
    let mut realtime = Timespec::ZERO;
    // SAFETY: all three timespecs are writable.
    let ok_calls = unsafe {
        clock_getres(CLOCK_MONOTONIC, &mut first) == 0
            && clock_getres(CLOCK_MONOTONIC, &mut second) == 0
            && clock_getres(CLOCK_REALTIME, &mut realtime) == 0
    };
    let first_ns = ns_of(&first);
    let second_ns = ns_of(&second);
    let realtime_ns = ns_of(&realtime);
    let ok = ok_calls
        && first_ns > 0
        && first_ns == second_ns
        && realtime_ns > 0
        && realtime_ns == first_ns;
    state.check(
        "getres",
        ok,
        format_args!(
            "mono_ns={} repeat_ns={} real_ns={} errno={}",
            first_ns,
            second_ns,
            realtime_ns,
            errno()
        ),
    );
}

fn row_monotonic(state: &mut State) {
    let mut previous = match clock_now(CLOCK_MONOTONIC) {
        Some(value) => value,
        None => {
            state.check(
                "monotonic",
                false,
                format_args!("first_errno={}", errno()),
            );
            return;
        }
    };
    let first = previous;
    let mut ok = first > 0;
    let mut failures = 0;
    for _ in 0..SAMPLES {
        let current = match clock_now(CLOCK_MONOTONIC) {
            Some(value) => value,
            None => {
                ok = false;
                failures += 1;
                break;
            }
        };
        if current < previous || current <= 0 {
            ok = false;
            failures += 1;
            break;
        }
        previous = current;
    }
    state.check(
        "monotonic",
        ok,
        format_args!(
            "samples={} first_ns={} last_ns={} failures={}",
            SAMPLES, first, previous, failures
        ),
    );
}

extern "C" fn smp_worker(argument: *mut c_void) -> *mut c_void {
    let failures = argument.cast::<i32>();
    let previous = match clock_now(CLOCK_MONOTONIC) {
        Some(value) if value > 0 => value,
        _ => {
            // SAFETY: `argument` points to the caller's stack failure counter.
            unsafe { *failures = 1 };
            return null_mut();
        }
    };
    let mut previous = previous;
    for _ in 0..SAMPLES {
        match clock_now(CLOCK_MONOTONIC) {
            Some(current) if current >= previous && current > 0 => previous = current,
            _ => {
                // SAFETY: `argument` points to the caller's stack failure counter.
                unsafe { *failures = 1 };
                return null_mut();
            }
        }
    }
    null_mut()
}

fn row_monotonic_smp(state: &mut State) {
    let mut first_failures: i32 = 0;
    let mut second_failures: i32 = 0;
    let mut first_thread: usize = 0;
    let mut second_thread: usize = 0;
    // SAFETY: the failure counters are unique stack locals live until both joins return.
    let first_created = unsafe {
        pthread_create(
            &mut first_thread,
            null_mut(),
            smp_worker,
            (&mut first_failures as *mut i32).cast::<c_void>(),
        )
    } == 0;
    // SAFETY: same as above for the second counter and thread slot.
    let second_created = unsafe {
        pthread_create(
            &mut second_thread,
            null_mut(),
            smp_worker,
            (&mut second_failures as *mut i32).cast::<c_void>(),
        )
    } == 0;
    let first_joined = first_created
        // SAFETY: `first_thread` is a thread the probe created.
        && unsafe { pthread_join(first_thread, null_mut()) } == 0;
    let second_joined = second_created
        // SAFETY: `second_thread` is a thread the probe created.
        && unsafe { pthread_join(second_thread, null_mut()) } == 0;
    let ok = first_created
        && second_created
        && first_joined
        && second_joined
        && first_failures == 0
        && second_failures == 0;
    state.check(
        "monotonic-smp",
        ok,
        format_args!(
            "threads=2 samples={} first_failures={} second_failures={} created={}{} joined={}{} errno={}",
            SAMPLES,
            first_failures,
            second_failures,
            flag(first_created),
            flag(second_created),
            flag(first_joined),
            flag(second_joined),
            errno()
        ),
    );
}

fn row_sleep(state: &mut State, mutation: bool) {
    let request_ns = if mutation { 1_000_000 } else { SLEEP_NS };
    let request = Timespec {
        tv_sec: request_ns / NANOS_PER_SECOND,
        tv_nsec: request_ns % NANOS_PER_SECOND,
    };
    let before = clock_now(CLOCK_MONOTONIC);
    // SAFETY: `request` is readable and a NULL remainder is allowed.
    let result = unsafe { nanosleep(&request, null_mut()) };
    let saved_errno = errno();
    let after = clock_now(CLOCK_MONOTONIC);
    let elapsed = match (before, after) {
        (Some(before), Some(after)) => after - before,
        _ => 0,
    };
    // The mutation requests 1 ms but keeps the 100 ms at-least assertion.
    let ok = result == 0 && elapsed >= SLEEP_NS;
    state.check(
        "sleep",
        ok,
        format_args!(
            "request_ns={} elapsed_ns={} minimum_ns={} rc={} errno={} mutation={}",
            request_ns,
            elapsed,
            SLEEP_NS,
            result,
            saved_errno,
            flag(mutation)
        ),
    );
}

fn row_eintr(state: &mut State) {
    if !install_signal(SIGUSR1) {
        state.check("eintr", false, format_args!("sigaction_errno={}", errno()));
        return;
    }
    SIGNAL_SEEN.store(0, Ordering::SeqCst);
    // SAFETY: fork has no preconditions and the probe handles both branches.
    let child = unsafe { fork() };
    if child == 0 {
        let delay = Timespec {
            tv_sec: 0,
            tv_nsec: SLEEP_NS,
        };
        // SAFETY: the delay is readable and the child owns its address space.
        unsafe {
            nanosleep(&delay, null_mut());
            kill(getppid(), SIGUSR1);
            _exit(0);
        }
    }
    if child < 0 {
        state.check("eintr", false, format_args!("fork_errno={}", errno()));
        return;
    }
    let request = Timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    let mut remaining = Timespec::ZERO;
    set_errno(0);
    // SAFETY: `request` is readable and `remaining` is writable.
    let result = unsafe { nanosleep(&request, &mut remaining) };
    let saved_errno = errno();
    let mut status: c_int = 0;
    // SAFETY: the child is the probe's own fork; the status pointer is writable.
    unsafe {
        waitpid(child, &mut status, 0);
    }
    let seen = SIGNAL_SEEN.load(Ordering::SeqCst);
    let left = ns_of(&remaining);
    let ok = result == -1
        && saved_errno == EINTR
        && seen > 0
        && left > 0
        && left < NANOS_PER_SECOND;
    state.check(
        "eintr",
        ok,
        format_args!(
            "rc={} errno={} signals={} remaining_ns={} max_ns={}",
            result, saved_errno, seen, left, NANOS_PER_SECOND
        ),
    );
}

fn row_gettimeofday(state: &mut State) {
    let mut realtime = Timespec::ZERO;
    let mut wall = Timeval::ZERO;
    // SAFETY: `realtime` and `wall` are writable; a NULL timezone is allowed.
    let ok_calls = unsafe {
        clock_gettime(CLOCK_REALTIME, &mut realtime) == 0
            && gettimeofday(&mut wall, null_mut()) == 0
    };
    let real_ns = ns_of(&realtime);
    let wall_ns = wall
        .tv_sec
        .saturating_mul(NANOS_PER_SECOND)
        .saturating_add(wall.tv_usec.saturating_mul(1_000));
    let delta = if real_ns > wall_ns {
        real_ns - wall_ns
    } else {
        wall_ns - real_ns
    };
    let ok = ok_calls && delta <= 50_000_000;
    state.check(
        "gettimeofday",
        ok,
        format_args!("real_ns={} wall_ns={} delta_ns={} errno={}", real_ns, wall_ns, delta, errno()),
    );
}

fn row_rates(state: &mut State) {
    let real_before = clock_now(CLOCK_REALTIME);
    let mono_before = clock_now(CLOCK_MONOTONIC);
    let request = Timespec {
        tv_sec: 0,
        tv_nsec: SLEEP_NS,
    };
    // SAFETY: `request` is readable and a NULL remainder is allowed.
    let result = unsafe { nanosleep(&request, null_mut()) };
    let saved_errno = errno();
    let real_after = clock_now(CLOCK_REALTIME);
    let mono_after = clock_now(CLOCK_MONOTONIC);
    let (real, mono) = match (real_before, real_after, mono_before, mono_after) {
        (Some(rb), Some(ra), Some(mb), Some(ma)) => (ra - rb, ma - mb),
        _ => (0, 0),
    };
    let difference = if real > mono { real - mono } else { mono - real };
    let ok = result == 0
        && real >= SLEEP_NS / 2
        && mono >= SLEEP_NS / 2
        && difference <= 50_000_000;
    state.check(
        "rates",
        ok,
        format_args!(
            "real_ns={} mono_ns={} minimum_ns={} difference_ns={} rc={} errno={}",
            real,
            mono,
            SLEEP_NS / 2,
            difference,
            result,
            saved_errno
        ),
    );
}

fn run_normal() -> i32 {
    let mut state = State::begin(TOPIC);
    row_ids(&mut state);
    row_getres(&mut state);
    row_monotonic(&mut state);
    row_monotonic_smp(&mut state);
    row_sleep(&mut state, false);
    row_eintr(&mut state);
    row_gettimeofday(&mut state);
    row_rates(&mut state);
    state.finish()
}

fn run_mutation(name: &str) -> i32 {
    let mut state = State {
        topic: TOPIC,
        total: 0,
        passed: 0,
        failed: 0,
    };
    conformance::mutation_begin(TOPIC, name);
    match name {
        "early-sleep" => row_sleep(&mut state, true),
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name)
}

fn c_str_eq(pointer: *const c_char, expected: &[u8]) -> bool {
    let mut cursor = pointer as *const u8;
    for byte in expected {
        // SAFETY: the caller supplies a NUL-terminated argv entry.
        if unsafe { *cursor } != *byte {
            return false;
        }
        // SAFETY: advance within the same NUL-terminated string.
        cursor = unsafe { cursor.add(1) };
    }
    // SAFETY: the expected text is a prefix; the next byte must end the string.
    unsafe { *cursor == 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut failures = run_normal();
    let mut index = 1;
    while index < argc {
        // SAFETY: index is inside argv.
        if c_str_eq(unsafe { *argv.add(index as usize) }, b"--all") {
            failures |= run_mutation("early-sleep");
        }
        index += 1;
    }
    if failures == 0 { 0 } else { 1 }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    conformance::emit(format_args!("CONFORM {TOPIC} panic\n"));
    // SAFETY: the process is aborting and `_exit` never returns.
    unsafe { _exit(101) }
}
