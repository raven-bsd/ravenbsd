// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `epoll_pwait(2)`.
use core::ffi::c_int;

use raven_libc_personality_linux::epoll::EpollEvent;
use raven_libc_personality_linux::seams::{Errno, SigSet};

use crate::personality_common::{EPOLL, RealEvq, RealFdTable, ret_count, ret_int};

/// Wait with an optional signal mask; a non-NULL mask is `ENOSYS` until PG-8.
///
/// # Safety
/// `events` points to `maxevents` writable records; `sigmask` is NULL or a `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_pwait(
    epfd: c_int,
    events: *mut EpollEvent,
    maxevents: c_int,
    timeout: c_int,
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
    // SAFETY: the caller granted `count` writable records.
    let out = unsafe { core::slice::from_raw_parts_mut(events, count) };
    ret_count(EPOLL.with(|table| {
        raven_libc_personality_linux::epoll::pwait(
            &mut RealEvq,
            &RealFdTable,
            table,
            epfd,
            out,
            timeout,
            mask,
        )
    }))
}
