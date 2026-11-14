// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_associate` (port.h).

use core::ffi::{c_int, c_void};

use raven_libc_personality_solaris::port;

use crate::personality_solaris::real::{PORTS, RealEvq, RealFds, ret_port};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn port_associate(
    port_fd: c_int,
    source: c_int,
    object: usize,
    events: c_int,
    user: *mut c_void,
) -> c_int {
    ret_port(PORTS.with(|table| {
        port::associate(
            table,
            &mut RealEvq,
            &RealFds,
            port_fd,
            source,
            object,
            events,
            user as usize,
        )
    }))
}
