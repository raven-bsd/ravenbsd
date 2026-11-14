// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_create` (port.h).

use core::ffi::c_int;

use raven_libc_personality_solaris::port;

use crate::personality_solaris::real::{PORTS, RealDoorbell, RealEvq, RealFds, ret_fd};

#[unsafe(no_mangle)]
pub extern "C" fn port_create() -> c_int {
    ret_fd(PORTS.with(|table| port::create(table, &mut RealEvq, &mut RealDoorbell, &mut RealFds)))
}
