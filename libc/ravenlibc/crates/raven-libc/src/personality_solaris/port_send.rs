// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_send` (port.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::port;

use crate::personality_solaris::real::{PORTS, RealDoorbell, RealFds, ret_port};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn port_send(port_fd: c_int, events: c_int, user: *mut c_void) -> c_int {
    ret_port(PORTS.with(|table| {
        port::send(
            table,
            &mut RealDoorbell,
            &RealFds,
            port_fd,
            events,
            user as usize,
        )
    }))
}
