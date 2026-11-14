// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `port_getn` (port.h).

use core::ffi::c_int;

use raven_libc_abi::structs::Timespec;
use raven_libc_personality_solaris::consts::PortEvent;
use raven_libc_personality_solaris::port;

use crate::ffi::errno::{set_platform, set_raw};
use crate::personality_solaris::real::{PORTS, RealEvq, RealFds, timespec};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn port_getn(
    port_fd: c_int,
    list: *mut PortEvent,
    max: core::ffi::c_uint,
    nget: *mut core::ffi::c_uint,
    timeout: *const Timespec,
) -> c_int {
    if list.is_null() || nget.is_null() {
        set_raw(raven_libc_abi::errno::EINVAL);
        return -1;
    }
    // SAFETY: the caller owns `max` writable `port_event_t` elements (C prototype).
    let out = unsafe { core::slice::from_raw_parts_mut(list, max as usize) };
    // SAFETY: `nget` is non-NULL and readable.
    let mut count = unsafe { *nget };
    let timeout = timespec(timeout);
    let result = PORTS.with(|table| {
        port::getn(
            table,
            &mut RealEvq,
            &RealFds,
            port_fd,
            out,
            &mut count,
            timeout.as_ref(),
        )
    });
    match result {
        Ok(()) => {
            // SAFETY: `nget` is non-NULL and writable.
            unsafe { *nget = count };
            0
        }
        Err(error) => {
            // SAFETY: `nget` is non-NULL and writable.
            unsafe { *nget = 0 };
            set_platform(error);
            -1
        }
    }
}
