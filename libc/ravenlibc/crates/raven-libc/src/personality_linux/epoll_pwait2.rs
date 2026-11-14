// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `epoll_pwait2(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Timespec;
use raven_libc_personality_linux::epoll::EpollEvent;
use raven_libc_personality_linux::seams::{Errno, SigSet};

use crate::personality_common::{EPOLL, RealEvq, RealFdTable, ret_count, ret_int, timespec};

/// Wait with a nanosecond timeout and an optional signal mask.
///
/// # Safety
/// `events` points to `maxevents` writable records; `timeout` and `sigmask` are NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_pwait2(
    epfd: c_int,
    events: *mut EpollEvent,
    maxevents: c_int,
    timeout: *const Timespec,
    sigmask: *const SigSet,
) -> c_int {
    if maxevents <= 0 {
        return ret_int(Err(Errno::EINVAL));
    }
    if events.is_null() {
        return ret_int(Err(Errno::EFAULT));
    }
    let Ok(count) = usize::try_from(maxevents) else {
        return ret_int(Err(Errno::EINVAL));
    };
    let mask = if sigmask.is_null() {
        None
    } else {
        // SAFETY: the caller's sigset_t contract.
        Some(unsafe { &*sigmask })
    };
    let ns = match unsafe { timespec(timeout) } {
        None => None,
        Some(value) => {
            if value.tv_sec < 0 || value.tv_nsec < 0 || value.tv_nsec >= 1_000_000_000 {
                return ret_int(Err(Errno::EINVAL));
            }
            let Some(total) = value
                .tv_sec
                .checked_mul(1_000_000_000)
                .and_then(|base| base.checked_add(value.tv_nsec))
            else {
                return ret_int(Err(Errno::EINVAL));
            };
            Some(total)
        }
    };
    // SAFETY: the caller granted `count` writable records.
    let out = unsafe { core::slice::from_raw_parts_mut(events, count) };
    ret_count(EPOLL.with(|table| {
        raven_libc_personality_linux::epoll::pwait2(
            &mut RealEvq,
            &RealFdTable,
            table,
            epfd,
            out,
            ns,
            mask,
        )
    }))
}
