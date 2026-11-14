// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `epoll_wait(2)`.
use core::ffi::c_int;

use raven_libc_personality_linux::epoll::EpollEvent;
use raven_libc_personality_linux::seams::Errno;

use crate::personality_common::{EPOLL, RealEvq, RealFdTable, ret_count, ret_int};

/// Wait for events.
///
/// # Safety
/// `events` points to `maxevents` writable `struct epoll_event` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_wait(
    epfd: c_int,
    events: *mut EpollEvent,
    maxevents: c_int,
    timeout: c_int,
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
    // SAFETY: the caller granted `count` writable records.
    let out = unsafe { core::slice::from_raw_parts_mut(events, count) };
    ret_count(EPOLL.with(|table| {
        raven_libc_personality_linux::epoll::wait(
            &mut RealEvq,
            &RealFdTable,
            table,
            epfd,
            out,
            timeout,
            None,
        )
    }))
}
