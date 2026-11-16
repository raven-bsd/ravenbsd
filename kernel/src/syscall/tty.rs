// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native controlling-terminal operations (`TIOCSCTTY`, `tcgetpgrp`, `tcsetpgrp`).
//!
//! The operations use kernel-owned raw numbers because the ABI crate is reviewer-owned and there
//! is no generic ioctl syscall yet. They operate on an existing scheme-file handle whose server
//! registered `ttyd` or `tty`; the kernel owns the session/foreground-group state.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::ipc::SchemeFile;
#[cfg(target_arch = "x86_64")]
use crate::ipc::SchemeFile;
use crate::process::handle::Object;
use crate::process::{self, tty};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// Raw `TIOCSCTTY(handle) -> 0`.
pub const PROC_CTTY_SET: usize = 0x1e0;
/// Raw `tcgetpgrp(handle, out: *mut i32) -> 0`.
pub const PROC_CTTY_GET: usize = 0x1e1;
/// Raw `tcsetpgrp(handle, pgrp) -> 0`.
pub const PROC_CTTY_SET_FG: usize = 0x1e2;

fn file_from_handle(
    caller: &Arc<process::Process>,
    handle: Handle,
) -> Result<Arc<SchemeFile>, Errno> {
    match caller.handles.lock().get(handle, Rights::NONE)? {
        Object::SchemeFile(file) => Ok(Arc::clone(file)),
        _ => Err(Errno::ENOTTY),
    }
}

/// `TIOCSCTTY`: acquire the handle's tty as the caller session's controlling terminal.
pub fn set_ctty(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = file_from_handle(&caller, handle)?;
    tty::acquire(&caller, &file)?;
    Ok(0)
}

/// `tcgetpgrp`: write the foreground process group of the handle's tty.
pub fn get_pgrp(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let file = file_from_handle(&caller, handle)?;
    let pgrp = tty::get_fg(&caller, &file)?;
    let destination = UserSlice::new(frame.rsi, 4)?;
    copy_to_user(destination, &pgrp.to_le_bytes())?;
    Ok(0)
}

/// `tcsetpgrp`: set the foreground process group of the handle's tty.
pub fn set_pgrp(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let pgrp = frame.rsi as u32 as i32;
    let file = file_from_handle(&caller, handle)?;
    tty::set_fg(&caller, &file, pgrp)?;
    Ok(0)
}
