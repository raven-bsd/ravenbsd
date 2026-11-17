// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! POSIX per-process timers over the kernel `TIMER_CREATE`/`TIMER_SET` interfaces.

use core::cell::UnsafeCell;
use core::ffi::{c_int, c_void};

use raven_abi::types::Handle;
use raven_libc_abi::errno::{EINVAL, Errno};
use raven_libc_abi::structs::{Itimerspec, Sigevent, Timespec};
use raven_libc_time::timer::{ITimerSpec, KernelOp, Notify, Now, TimerId, TimerTable};

use super::ShellTime;

const SIGEV_SIGNAL: i32 = 0;
const SIGEV_NONE: i32 = 1;
const SIGEV_THREAD: i32 = 2;
const TIMER_ABSTIME: c_int = 1;
const CLOCK_REALTIME: i32 = 0;
const CLOCK_MONOTONIC: i32 = 1;

struct TimerState {
    table: TimerTable,
    handles: alloc::vec::Vec<(u64, Handle)>,
}

struct Registry(UnsafeCell<Option<TimerState>>);

// SAFETY: the shell is single-threaded until the timer thread support lands.
unsafe impl Sync for Registry {}

static TIMERS: Registry = Registry(UnsafeCell::new(None));

fn state() -> &'static mut TimerState {
    // SAFETY: single-threaded registry cell; first use initialises the table.
    let slot = unsafe { &mut *TIMERS.0.get() };
    if slot.is_none() {
        *slot = Some(TimerState {
            table: TimerTable::new(64),
            handles: alloc::vec::Vec::new(),
        });
    }
    slot.as_mut().expect("timer state initialised")
}

fn to_ns(value: &Timespec) -> u64 {
    if value.tv_sec < 0 {
        return 0;
    }
    (value.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(value.tv_nsec.max(0) as u64)
}

fn now(clock: i32) -> Now {
    let mono = raven_libc_time::clock::clock_gettime(&ShellTime, CLOCK_MONOTONIC)
        .unwrap_or(Timespec::ZERO);
    let clock_value =
        raven_libc_time::clock::clock_gettime(&ShellTime, clock).unwrap_or(Timespec::ZERO);
    Now {
        clock: clock_value,
        mono_ns: to_ns(&mono),
    }
}

fn kernel_clock(clock: raven_libc_time::sys::ClockId) -> u32 {
    match clock {
        raven_libc_time::sys::ClockId::Realtime => CLOCK_REALTIME as u32,
        raven_libc_time::sys::ClockId::Monotonic => CLOCK_MONOTONIC as u32,
    }
}

fn fail(error: Errno) -> c_int {
    crate::ffi::errno::set(error);
    -1
}

fn decode(timer: *mut c_void) -> u64 {
    timer as usize as u64
}

fn handle_for(state: &TimerState, id: u64) -> Option<Handle> {
    state
        .handles
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .map(|(_, handle)| *handle)
}

fn notify_from(event: *const Sigevent) -> Notify {
    if event.is_null() {
        return Notify::Signal {
            signo: 14,
            value: 0,
        };
    }
    // SAFETY: the caller's readable sigevent.
    let bytes = unsafe { (*event).bytes };
    let value = bytes.first().copied().unwrap_or(0) as i64;
    let signo = bytes.get(1).copied().unwrap_or(0) as i32;
    let notify = (bytes.get(1).copied().unwrap_or(0) >> 32) as i32;
    match notify {
        SIGEV_SIGNAL => Notify::Signal { signo, value },
        SIGEV_THREAD => Notify::Thread,
        SIGEV_NONE => Notify::None,
        _ => Notify::None,
    }
}

/// `timer_create`.
///
/// # Safety
/// `event` is NULL or readable; `timer_id` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_create(
    clock_id: i32,
    event: *const Sigevent,
    timer_id: *mut *mut c_void,
) -> c_int {
    if timer_id.is_null() {
        return fail(Errno(EINVAL as u16));
    }
    let notify = notify_from(event);
    let state = state();
    let (id, operation) = match state.table.create(clock_id, notify) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let KernelOp::Create { clock } = operation else {
        return fail(Errno(EINVAL as u16));
    };
    let handle = match raven_libc_sys::sys::timer_create(kernel_clock(clock), 0) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = state.table.delete(id);
            return fail(Errno(error.0 as u16));
        }
    };
    state.handles.push((id.0, handle));
    // SAFETY: the caller's writable timer_t slot.
    unsafe { *timer_id = id.0 as usize as *mut c_void };
    0
}

/// `timer_delete`.
///
/// # Safety
/// `timer` is a timer_t returned by `timer_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_delete(timer: *mut c_void) -> c_int {
    let state = state();
    let id = decode(timer);
    let Some(index) = state
        .handles
        .iter()
        .position(|(candidate, _)| *candidate == id)
    else {
        return fail(Errno(EINVAL as u16));
    };
    let handle = state.handles.get(index).map(|(_, handle)| *handle);
    match state.table.delete(TimerId(id)) {
        Ok(KernelOp::Close) => {
            if let Some(handle) = handle {
                let _ = raven_libc_sys::sys::handle_close(handle);
            }
            state.handles.remove(index);
            0
        }
        Ok(_) => fail(Errno(EINVAL as u16)),
        Err(error) => fail(error),
    }
}

/// `timer_settime`.
///
/// # Safety
/// `new` is readable; `old` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_settime(
    timer: *mut c_void,
    flags: c_int,
    new_value: *const Itimerspec,
    old_value: *mut Itimerspec,
) -> c_int {
    if new_value.is_null() {
        return fail(Errno(EINVAL as u16));
    }
    let state = state();
    let id = TimerId(decode(timer));
    let Some(handle) = handle_for(state, id.0) else {
        return fail(Errno(EINVAL as u16));
    };
    // SAFETY: the caller's readable itimerspec.
    let new = unsafe { *new_value };
    let spec = ITimerSpec {
        interval: new.it_interval,
        value: new.it_value,
    };
    let clock = if flags & TIMER_ABSTIME != 0 {
        CLOCK_REALTIME
    } else {
        CLOCK_MONOTONIC
    };
    match state
        .table
        .settime(id, flags & TIMER_ABSTIME != 0, spec, now(clock))
    {
        Ok((old, operation)) => {
            if let KernelOp::Set {
                initial_ns,
                interval_ns,
            } = operation
            {
                if let Err(error) = raven_libc_sys::sys::timer_set(handle, initial_ns, interval_ns)
                {
                    return fail(Errno(error.0 as u16));
                }
            }
            if !old_value.is_null() {
                // SAFETY: the caller's writable itimerspec.
                unsafe {
                    *old_value = Itimerspec {
                        it_interval: old.interval,
                        it_value: old.value,
                    };
                }
            }
            0
        }
        Err(error) => fail(error),
    }
}

/// `timer_gettime`.
///
/// # Safety
/// `value` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_gettime(timer: *mut c_void, value: *mut Itimerspec) -> c_int {
    if value.is_null() {
        return fail(Errno(EINVAL as u16));
    }
    let state = state();
    let id = TimerId(decode(timer));
    if handle_for(state, id.0).is_none() {
        return fail(Errno(EINVAL as u16));
    }
    let now = now(CLOCK_MONOTONIC);
    match state.table.gettime(id, now.mono_ns) {
        Ok(spec) => {
            // SAFETY: the caller's writable itimerspec.
            unsafe {
                *value = Itimerspec {
                    it_interval: spec.interval,
                    it_value: spec.value,
                };
            }
            0
        }
        Err(error) => fail(error),
    }
}

/// `timer_getoverrun` body.
///
/// # Safety
/// `timer` is a live timer_t.
pub(crate) unsafe fn timer_getoverrun_impl(timer: *mut c_void) -> c_int {
    let state = state();
    let id = TimerId(decode(timer));
    if handle_for(state, id.0).is_none() {
        return fail(Errno(EINVAL as u16));
    }
    i32::try_from(state.table.getoverrun(id)).unwrap_or(i32::MAX)
}
