// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_get` (port.h).

use core::ffi::c_int;

use raven_libc_abi::structs::Timespec;
use raven_libc_personality_solaris::consts::PortEvent;
use raven_libc_personality_solaris::port;

use crate::ffi::errno::set_platform;
use crate::personality_solaris::real::{PORTS, RealEvq, RealFds, timespec};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn port_get(
    port_fd: c_int,
    event: *mut PortEvent,
    timeout: *const Timespec,
) -> c_int {
    if event.is_null() {
        return crate::personality_solaris::real::invalid();
    }
    let timeout = timespec(timeout);
    match PORTS.with(|table| port::get(table, &mut RealEvq, &RealFds, port_fd, timeout.as_ref())) {
        Ok(value) => {
            // SAFETY: the caller passed a writable `port_event_t` (C prototype).
            unsafe { *event = value };
            0
        }
        Err(error) => {
            set_platform(error);
            -1
        }
    }
}
