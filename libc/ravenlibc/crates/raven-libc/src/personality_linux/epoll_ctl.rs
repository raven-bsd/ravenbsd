// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `epoll_ctl(2)`.
use core::ffi::c_int;

use raven_libc_personality_linux::epoll::EpollEvent;
use raven_libc_personality_linux::seams::Errno;

use crate::personality_common::{EPOLL, RealEvq, RealFdTable, ret_unit};

/// Control one registration.
///
/// # Safety
/// `event` is NULL or points to a readable `struct epoll_event`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn epoll_ctl(
    epfd: c_int,
    op: c_int,
    fd: c_int,
    event: *const EpollEvent,
) -> c_int {
    let value = if event.is_null() {
        None
    } else {
        // SAFETY: the caller's epoll_event contract; the record may be packed.
        Some(unsafe { core::ptr::read_unaligned(event) })
    };
    let result = EPOLL.with(|table| {
        raven_libc_personality_linux::epoll::ctl(
            &mut RealEvq,
            &mut RealFdTable,
            table,
            epfd,
            op,
            fd,
            value.as_ref(),
        )
    });
    if result.is_err()
        && event.is_null()
        && op != raven_libc_personality_linux::epoll::EPOLL_CTL_DEL
    {
        return ret_unit(Err(Errno::EFAULT));
    }
    ret_unit(result)
}
