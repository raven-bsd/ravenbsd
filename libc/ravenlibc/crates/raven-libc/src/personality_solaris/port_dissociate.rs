// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_dissociate` (port.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::port;

use crate::personality_solaris::real::{PORTS, RealEvq, RealFds, ret_port};

#[unsafe(no_mangle)]
pub extern "C" fn port_dissociate(port_fd: c_int, source: c_int, object: usize) -> c_int {
    ret_port(
        PORTS
            .with(|table| port::dissociate(table, &mut RealEvq, &RealFds, port_fd, source, object)),
    )
}
