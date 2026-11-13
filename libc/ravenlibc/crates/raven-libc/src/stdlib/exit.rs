// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `exit(3)`.
use core::ffi::c_int;

use crate::unistd as io_core;

use raven_libc_sys::weak_hook;

fn nothing() {}

weak_hook! {
    /// Run the registered `atexit` handlers (C41's registry; C61 owns its instance).
    pub(crate) fn __rl_funcs_on_exit() -> ();
    dummy = crate::stdlib::exit::nothing;
}

weak_hook! {
    /// Flush and close the stdio streams.
    pub(crate) fn __rl_stdio_exit() -> ();
    dummy = crate::stdlib::exit::nothing;
}

/// Terminate the process after the exit handlers.
#[unsafe(no_mangle)]
pub extern "C" fn exit(status: c_int) -> ! {
    __rl_funcs_on_exit();
    __rl_stdio_exit();
    io_core::_exit::_exit(status)
}
