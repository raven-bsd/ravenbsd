// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `setitimer`/`getitimer` C ABI over the shared `ITIMER_REAL` state.
//!
//! `struct itimerval` is two POSIX `timeval`s (`it_interval`, `it_value`). The first-party
//! `raven-libc-time` `ITimer` model validates the selector, the time ranges, the old value and
//! the zero-value cancellation; the shell arms the one kernel timer through `time::alarm`.
use core::ffi::c_int;

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::{Timespec, Timeval};
use raven_libc_time::itimer::ITimer;
use raven_libc_time::timer::ITimerSpec;

/// POSIX `struct itimerval`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ITimerVal {
    /// Repeating interval.
    pub it_interval: Timeval,
    /// Time to the next expiration.
    pub it_value: Timeval,
}

const _: () = assert!(core::mem::size_of::<ITimerVal>() == 32);
const _: () = assert!(core::mem::align_of::<ITimerVal>() == 8);
const _: () = assert!(core::mem::offset_of!(ITimerVal, it_interval) == 0);
const _: () = assert!(core::mem::offset_of!(ITimerVal, it_value) == 16);

/// Convert one `timeval` and reject out-of-range fields.
fn timeval(value: &Timeval) -> Result<Timespec, Errno> {
    ITimer::timeval_to_timespec(value.tv_sec, value.tv_usec)
}

/// Convert the C request into the model's spec.
fn spec_from(value: &ITimerVal) -> Result<ITimerSpec, Errno> {
    Ok(ITimerSpec {
        interval: timeval(&value.it_interval)?,
        value: timeval(&value.it_value)?,
    })
}

fn timeval_of(time: Timespec) -> Timeval {
    Timeval {
        tv_sec: time.tv_sec,
        tv_usec: time.tv_nsec / 1_000,
    }
}

fn value_of(spec: ITimerSpec) -> ITimerVal {
    ITimerVal {
        it_interval: timeval_of(spec.interval),
        it_value: timeval_of(spec.value),
    }
}

fn set_errno(error: Errno) {
    crate::ffi::errno::set(error);
}

/// `setitimer`.
///
/// # Safety
/// `new` is a readable `struct itimerval`; `old` is NULL or writable for one.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setitimer(
    which: c_int,
    new: *const ITimerVal,
    old: *mut ITimerVal,
) -> c_int {
    if new.is_null() {
        set_errno(Errno(EFAULT as u16));
        return -1;
    }
    // SAFETY: the caller supplies a readable `struct itimerval` for this call.
    let spec = match spec_from(unsafe { &*new }) {
        Ok(spec) => spec,
        Err(error) => {
            set_errno(error);
            return -1;
        }
    };
    match crate::time::alarm::set_spec(which, spec) {
        Ok(previous) => {
            if !old.is_null() {
                // SAFETY: `old` is a caller-owned writable `struct itimerval`.
                unsafe { core::ptr::write(old, value_of(previous)) };
            }
            0
        }
        Err(error) => {
            set_errno(error);
            -1
        }
    }
}

/// `getitimer`.
///
/// # Safety
/// `curr` is a writable `struct itimerval`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getitimer(which: c_int, curr: *mut ITimerVal) -> c_int {
    if curr.is_null() {
        set_errno(Errno(EFAULT as u16));
        return -1;
    }
    match crate::time::alarm::get_spec(which) {
        Ok(spec) => {
            // SAFETY: `curr` is a caller-owned writable `struct itimerval`.
            unsafe { core::ptr::write(curr, value_of(spec)) };
            0
        }
        Err(error) => {
            set_errno(error);
            -1
        }
    }
}

#[cfg(test)]
#[path = "setitimer_ut.rs"]
mod tests;
