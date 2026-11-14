// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `timerfd_gettime(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Itimerspec;
use raven_libc_personality_linux::seams::Errno;

use crate::personality_common::{RealFdTable, RealTimer, TIMER_FDS, ret_unit};

/// Read the remaining/interval values; `ENOSYS` until PG-19.
///
/// # Safety
/// `old` points to one writable `itimerspec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timerfd_gettime(fd: c_int, old: *mut Itimerspec) -> c_int {
    if old.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    let result = TIMER_FDS.with(|state| {
        raven_libc_personality_linux::timerfd::gettime(&RealFdTable, &mut RealTimer, state, fd)
    });
    if let Ok((initial, interval)) = result {
        let value = Itimerspec {
            it_interval: raven_libc_abi::structs::Timespec {
                tv_sec: i64::try_from(interval / 1_000_000_000).unwrap_or(i64::MAX),
                tv_nsec: i64::try_from(interval % 1_000_000_000).unwrap_or(0),
            },
            it_value: raven_libc_abi::structs::Timespec {
                tv_sec: i64::try_from(initial / 1_000_000_000).unwrap_or(i64::MAX),
                tv_nsec: i64::try_from(initial % 1_000_000_000).unwrap_or(0),
            },
        };
        // SAFETY: the caller granted one writable itimerspec.
        unsafe { core::ptr::write_unaligned(old, value) };
    }
    ret_unit(result.map(|_| ()))
}
