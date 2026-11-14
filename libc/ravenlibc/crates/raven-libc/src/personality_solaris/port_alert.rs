// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_alert` (port.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::port;

use crate::personality_solaris::real::{RealFds, ret_port};

#[unsafe(no_mangle)]
pub extern "C" fn port_alert(
    port_fd: c_int,
    _flags: c_int,
    _events: c_int,
    _user: *mut c_void,
) -> c_int {
    ret_port(port::alert(&RealFds, port_fd))
}
