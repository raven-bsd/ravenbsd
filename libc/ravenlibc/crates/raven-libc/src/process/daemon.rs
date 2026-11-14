// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `daemon(3)`.
use core::ffi::c_int;

/// Detach from the terminal through C53's step model.
#[unsafe(no_mangle)]
pub extern "C" fn daemon(nochdir: c_int, noclose: c_int) -> c_int {
    let mut ops = crate::process::daemon_ops::ShellDaemon;
    match raven_libc_proc::daemon::daemon(&mut ops, nochdir != 0, noclose != 0) {
        Ok(raven_libc_proc::daemon::DaemonStep::ParentExit) => raven_libc_sys::sys::exit(0),
        Ok(raven_libc_proc::daemon::DaemonStep::ChildReady) => 0,
        Err(error) => crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error))),
    }
}
