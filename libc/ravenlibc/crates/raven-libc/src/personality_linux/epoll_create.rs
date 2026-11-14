// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `epoll_create(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealEvq, RealFdTable, ret_int};

/// Create an epoll instance; the size is validated and otherwise ignored.
#[unsafe(no_mangle)]
pub extern "C" fn epoll_create(size: c_int) -> c_int {
    ret_int(raven_libc_personality_linux::epoll::create(
        &mut RealEvq,
        &mut RealFdTable,
        size,
    ))
}
