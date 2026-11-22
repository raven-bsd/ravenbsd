// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `alarm`/`ualarm`/`setitimer`/`getitimer`: one libc `ITIMER_REAL` timer and the SIGALRM
//! delivery thread.
//!
//! The kernel timer object is an event-queue source, not an asynchronous signal. One detached
//! libc thread waits on the timer event queue and queues `SIGALRM` through the kernel
//! `PROC_SIGNAL` syscall; `pause`/`PROC_SIGRECV` consumes the queued signal. The deadline and
//! interval arithmetic is the first-party `raven-libc-time` `ITimer` model, shared by all four
//! entry points so `alarm(2)` is exactly `setitimer(ITIMER_REAL, {n, 0})`.

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use raven_abi::types::Handle;
use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Timespec;
use raven_libc_abi::time::ITIMER_REAL;
use raven_libc_sys::sys::{EvqEvent, evq_create, evq_ctl, evq_wait, timer_create, timer_set};
use raven_libc_time::itimer::ITimer;
use raven_libc_time::timer::{ITimerSpec, Now};

use crate::ffi::lock::SpinLock;

const SIGALRM: i32 = 14;
const CLOCK_MONOTONIC: u32 = 1;
const CLOCK_REALTIME: u32 = 0;
const EVQ_ADD: u32 = 1;
const EVQ_SIGNALED: u32 = 8;

struct State {
    lock: SpinLock,
    timer: UnsafeCell<ITimer>,
}

// SAFETY: `lock` serialises every access to the timer state; the delivery thread only reads the
// atomics.
unsafe impl Sync for State {}

static STATE: State = State {
    lock: SpinLock::new(),
    timer: UnsafeCell::new(ITimer::new()),
};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static INTERVAL_NS: AtomicU64 = AtomicU64::new(0);
static TIMER: AtomicU32 = AtomicU32::new(0);
static READY: AtomicBool = AtomicBool::new(false);
static STARTING: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn pthread_create(
        thread: *mut usize,
        attr: *const c_void,
        start: Option<extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> c_int;
    fn pthread_detach(thread: usize) -> c_int;
}

fn now_ns() -> u64 {
    match raven_libc_sys::sys::clock_get(CLOCK_MONOTONIC) {
        Ok(time) => (time.tv_sec.max(0) as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(time.tv_nsec.max(0) as u64),
        Err(_) => 0,
    }
}

fn realtime_now() -> Timespec {
    match raven_libc_sys::sys::clock_get(CLOCK_REALTIME) {
        // The syscall returns the kernel's `Timespec`; the model uses the libc ABI record.
        Ok(time) => Timespec {
            tv_sec: time.tv_sec,
            tv_nsec: time.tv_nsec,
        },
        Err(_) => Timespec::ZERO,
    }
}

fn ns_of(time: Timespec) -> u64 {
    (time.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(time.tv_nsec.max(0) as u64)
}

fn timespec_of(ns: u64) -> Timespec {
    Timespec {
        tv_sec: (ns / 1_000_000_000) as i64,
        tv_nsec: (ns % 1_000_000_000) as i64,
    }
}

fn ceil_div(value: u64, unit: u64) -> u32 {
    let rounded = value
        .saturating_add(unit.saturating_sub(1))
        .checked_div(unit)
        .unwrap_or(0);
    u32::try_from(rounded).unwrap_or(u32::MAX)
}

fn arm(initial_ns: u64, interval_ns: u64) {
    let timer = TIMER.load(Ordering::Acquire);
    if timer != 0 {
        let _ = timer_set(Handle(timer), initial_ns, interval_ns);
    }
}

fn start_timer() -> bool {
    let Ok(evq) = evq_create(0) else {
        return false;
    };
    let Ok(timer) = timer_create(CLOCK_MONOTONIC, 0) else {
        return false;
    };
    if evq_ctl(evq, EVQ_ADD, timer, EVQ_SIGNALED, 0).is_err() {
        return false;
    }
    TIMER.store(timer.0, Ordering::Release);
    let mut thread = 0usize;
    // SAFETY: the thread entry is a valid C function and the arg is a scalar handle.
    let result = unsafe {
        pthread_create(
            &mut thread,
            ptr::null(),
            Some(timer_thread),
            evq.0 as usize as *mut c_void,
        )
    };
    if result != 0 {
        return false;
    }
    // SAFETY: the thread value came from a successful pthread_create.
    unsafe {
        let _ = pthread_detach(thread);
    }
    true
}

fn ensure_timer() {
    if READY.load(Ordering::Acquire) || STARTING.swap(true, Ordering::AcqRel) {
        while STARTING.load(Ordering::Acquire) && !READY.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        return;
    }
    let ready = start_timer();
    READY.store(ready, Ordering::Release);
    STARTING.store(false, Ordering::Release);
}

extern "C" fn timer_thread(arg: *mut c_void) -> *mut c_void {
    let evq = Handle(arg as usize as u32);
    let mut events = [EvqEvent::default(); 1];
    loop {
        let Ok(count) = evq_wait(evq, &mut events, 0) else {
            return ptr::null_mut();
        };
        if count == 0 || !ACTIVE.load(Ordering::Acquire) {
            continue;
        }
        if INTERVAL_NS.load(Ordering::Acquire) == 0 {
            ACTIVE.store(false, Ordering::Release);
        }
        let _ = raven_libc_sys::signal::proc_signal(crate::process::getpid::getpid(), SIGALRM);
    }
}

/// Arm or cancel one interval timer and return the previous value.
///
/// Supports `ITIMER_REAL`; the model returns `EINVAL` for the other selectors.
pub(crate) fn set_spec(which: i32, spec: ITimerSpec) -> Result<ITimerSpec, Errno> {
    ensure_timer();
    let now = Now {
        clock: realtime_now(),
        mono_ns: now_ns(),
    };
    STATE.lock.acquire();
    // SAFETY: the lock above serialises every access to the timer cell.
    let timer = unsafe { &mut *STATE.timer.get() };
    let result = timer.setitimer(which, spec, now);
    STATE.lock.release();
    let previous = result?;
    let initial_ns = ns_of(spec.value);
    let interval_ns = ns_of(spec.interval);
    INTERVAL_NS.store(interval_ns, Ordering::Release);
    ACTIVE.store(initial_ns != 0, Ordering::Release);
    arm(initial_ns, interval_ns);
    Ok(previous)
}

/// Read one interval timer without changing it.
pub(crate) fn get_spec(which: i32) -> Result<ITimerSpec, Errno> {
    STATE.lock.acquire();
    // SAFETY: the lock above serialises every access to the timer cell.
    let timer = unsafe { &*STATE.timer.get() };
    let result = timer.getitimer(which, now_ns());
    STATE.lock.release();
    result
}

/// Arm `alarm(seconds)` and return the previous remaining seconds.
pub fn alarm(seconds: u32) -> u32 {
    let spec = ITimerSpec {
        interval: Timespec::ZERO,
        value: timespec_of(u64::from(seconds).saturating_mul(1_000_000_000)),
    };
    match set_spec(ITIMER_REAL, spec) {
        Ok(previous) => ceil_div(ns_of(previous.value), 1_000_000_000),
        Err(_) => 0,
    }
}

/// Arm `ualarm(usec, interval_usec)` and return the previous remaining microseconds.
pub fn ualarm(usec: u32, interval_usec: u32) -> u32 {
    let spec = ITimerSpec {
        interval: timespec_of(u64::from(interval_usec).saturating_mul(1_000)),
        value: timespec_of(u64::from(usec).saturating_mul(1_000)),
    };
    match set_spec(ITIMER_REAL, spec) {
        Ok(previous) => ceil_div(ns_of(previous.value), 1_000),
        Err(_) => 0,
    }
}

/// `pause`: block until a queued signal arrives, then return `EINTR`.
pub fn pause() -> c_int {
    let mut out = [0u64; 3];
    match raven_libc_sys::signal::proc_sigrecv(&mut out, 0) {
        Ok(_) => {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::EINTR as u16,
            ));
            -1
        }
        Err(error) => {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(error.0));
            -1
        }
    }
}
