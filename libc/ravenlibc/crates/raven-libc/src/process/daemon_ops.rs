// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The process and platform operations C53's `daemon` model needs.

use raven_libc_proc::backend::ProcBackend;
use raven_libc_proc::daemon::{DaemonFork, DaemonOps};
use raven_libc_proc::errors::Errno;

fn platform_error(value: raven_libc_platform::Errno) -> Errno {
    i32::from(value.0)
}

/// The one `daemon` operation set.
pub(crate) struct ShellDaemon;

impl DaemonOps for ShellDaemon {
    fn fork(&mut self) -> Result<DaemonFork, Errno> {
        let backend = crate::ffi::procbackend::backend();
        let pid = crate::ffi::map_proc::fork(&backend)?;
        Ok(if pid == 0 {
            DaemonFork::Child
        } else {
            DaemonFork::Parent
        })
    }

    fn setsid(&mut self) -> Result<(), Errno> {
        crate::ffi::procbackend::backend().setsid().map(|_| ())
    }

    fn chdir_root(&mut self) -> Result<(), Errno> {
        crate::ffi::platform::global()
            .chdir(b"/")
            .map_err(platform_error)
    }

    fn open_devnull(&mut self) -> Result<i32, Errno> {
        crate::ffi::platform::global()
            .open(b"/dev/null", raven_libc_abi::fcntl::O_RDWR as u32, 0)
            .map_err(platform_error)
    }

    fn dup2(&mut self, fd: i32, newfd: i32) -> Result<(), Errno> {
        crate::ffi::platform::global()
            .dup2(fd, newfd, false)
            .map(|_| ())
            .map_err(platform_error)
    }

    fn close(&mut self, fd: i32) -> Result<(), Errno> {
        crate::ffi::platform::global()
            .close(fd)
            .map_err(platform_error)
    }
}
