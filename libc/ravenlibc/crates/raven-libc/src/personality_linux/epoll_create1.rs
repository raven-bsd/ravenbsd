// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `epoll_create1(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealEvq, RealFdTable, ret_int};

/// Create an epoll instance; only `EPOLL_CLOEXEC` is accepted.
#[unsafe(no_mangle)]
pub extern "C" fn epoll_create1(flags: c_int) -> c_int {
    ret_int(raven_libc_personality_linux::epoll::create1(
        &mut RealEvq,
        &mut RealFdTable,
        flags,
    ))
}
