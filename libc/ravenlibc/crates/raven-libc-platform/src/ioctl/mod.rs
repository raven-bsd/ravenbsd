// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ioctl` dispatch: tty protocol, scalar fd-table requests and the `net:` escape.

pub mod net;
pub mod tty;

pub use net::{NetIoctl, net_ioctl};
pub use tty::{
    cfgetispeed, cfgetospeed, cfmakeraw, cfsetispeed, cfsetospeed, tcflush, tcgetattr, tcsetattr,
};

use raven_abi::errno::Errno;
use raven_libc_abi::structs::Termios;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;
use crate::xlat::{self, IoctlAction};

/// The scalar `ioctl(fd, request, value)` form; pointer requests use the typed helpers.
pub fn ioctl<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    request: u32,
    value: i32,
) -> Result<i32, Errno> {
    match xlat::ioctl_action(request) {
        IoctlAction::FionBio => {
            let reference = platform.get(fd)?;
            let status = reference.status();
            let updated = if value != 0 {
                status | 0o4000
            } else {
                status & !0o4000
            };
            reference.set_status(updated);
            Ok(0)
        }
        IoctlAction::Fioclex => {
            platform.set_cloexec(fd, true)?;
            Ok(0)
        }
        IoctlAction::Fionclex => {
            platform.set_cloexec(fd, false)?;
            Ok(0)
        }
        IoctlAction::TiocGWinsz | IoctlAction::FionRead => Err(Errno::ENOTTY),
        IoctlAction::Unknown => Err(Errno::ENOTTY),
        // Pointer-bearing requests need `&mut Termios`; the shell calls `tcgetattr`.
        IoctlAction::TcGets | IoctlAction::TcSets | IoctlAction::TcSetSw | IoctlAction::TcSetSf => {
            Err(Errno::EINVAL)
        }
    }
}

/// `ioctl(fd, TCGETS, &t)`.
pub fn ioctl_tcgets<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
) -> Result<Termios, Errno> {
    tty::tcgetattr(platform, fd)
}

/// `ioctl(fd, TCSETS, &t)`.
pub fn ioctl_tcsets<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    termios: &Termios,
) -> Result<(), Errno> {
    tty::tcsetattr(platform, fd, termios)
}

#[cfg(test)]
#[path = "ioctl_ut.rs"]
mod tests;
