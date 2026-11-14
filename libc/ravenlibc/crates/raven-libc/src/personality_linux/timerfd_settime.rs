// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `timerfd_settime(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::{Itimerspec, Timespec};
use raven_libc_personality_linux::seams::Errno;

use crate::personality_common::{RealFdTable, RealTimer, TIMER_FDS, ret_unit};

fn to_ns(value: Timespec) -> Result<u64, Errno> {
    if value.tv_sec < 0 || value.tv_nsec < 0 || value.tv_nsec >= 1_000_000_000 {
        return Err(Errno::EINVAL);
    }
    let sec = u64::try_from(value.tv_sec).map_err(|_| Errno::EINVAL)?;
    let nsec = u64::try_from(value.tv_nsec).map_err(|_| Errno::EINVAL)?;
    Ok(sec.saturating_mul(1_000_000_000).saturating_add(nsec))
}

fn from_ns(ns: u64) -> Timespec {
    Timespec {
        tv_sec: i64::try_from(ns / 1_000_000_000).unwrap_or(i64::MAX),
        tv_nsec: i64::try_from(ns % 1_000_000_000).unwrap_or(0),
    }
}

/// Arm or disarm a timerfd.
///
/// # Safety
/// `new` points to a readable `itimerspec`; `old` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timerfd_settime(
    fd: c_int,
    flags: c_int,
    new: *const Itimerspec,
    old: *mut Itimerspec,
) -> c_int {
    if new.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller's itimerspec contract.
    let spec = unsafe { core::ptr::read_unaligned(new) };
    let initial = match to_ns(spec.it_value) {
        Ok(value) => value,
        Err(error) => return ret_unit(Err(error)),
    };
    let interval = match to_ns(spec.it_interval) {
        Ok(value) => value,
        Err(error) => return ret_unit(Err(error)),
    };
    let previous = TIMER_FDS.with(|state| state.get(fd).copied());
    let result = TIMER_FDS.with(|state| {
        raven_libc_personality_linux::timerfd::settime(
            &RealFdTable,
            &mut RealTimer,
            state,
            fd,
            flags,
            initial,
            interval,
        )
    });
    if result.is_ok() && !old.is_null() {
        let value = match previous {
            Some(timer) => Itimerspec {
                it_interval: from_ns(timer.interval_ns),
                it_value: from_ns(timer.initial_ns),
            },
            None => Itimerspec {
                it_interval: Timespec::ZERO,
                it_value: Timespec::ZERO,
            },
        };
        // SAFETY: the caller granted one writable itimerspec.
        unsafe { core::ptr::write_unaligned(old, value) };
    }
    ret_unit(result)
}
