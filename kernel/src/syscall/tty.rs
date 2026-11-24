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
use crate::cmdline::CmdLine;
#[cfg(target_arch = "x86_64")]
use crate::ipc::SchemeFile;
use crate::kprintln;
use crate::memory::paging;
use crate::process::handle::Object;
use crate::process::{self, tty};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// K495 rev180 diagnostic gate: the pty gate image boots with `k495-debug=1`.
fn debug_enabled() -> bool {
    CmdLine::parse(paging::cmdline())
        .get("k495-debug")
        .is_some()
}

/// Raw `TIOCSCTTY(handle) -> 0`.
pub const PROC_CTTY_SET: usize = 0x1e0;
/// Raw `tcgetpgrp(handle, out: *mut i32) -> 0`.
pub const PROC_CTTY_GET: usize = 0x1e1;
/// Raw `tcsetpgrp(handle, pgrp) -> 0`.
pub const PROC_CTTY_SET_FG: usize = 0x1e2;
/// Raw `proc_ctty_handle() -> handle`.
pub const PROC_CTTY_HANDLE: usize = 0x1e3;

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
    if debug_enabled() {
        let ids = *caller.ids.lock();
        kprintln!(
            "ctty: PROC_CTTY_SET pid={} sid={} pgid={} handle={} server={} file={} tty={}",
            caller.id,
            ids.sid,
            ids.pgid,
            handle.0,
            file.server.id,
            file.server_id(),
            tty::is_tty_file(&file)
        );
    }
    let result = tty::acquire(&caller, &file);
    if debug_enabled() {
        match &result {
            Ok(()) => kprintln!("ctty: PROC_CTTY_SET pid={} result=ok", caller.id),
            Err(error) => {
                kprintln!(
                    "ctty: PROC_CTTY_SET pid={} result=err({:?})",
                    caller.id,
                    error
                )
            }
        }
    }
    result?;
    Ok(0)
}

/// `proc_ctty_handle`: a new handle for the caller session's controlling terminal.
///
/// `/dev/tty` is not a VFS node: it names the process's controlling terminal. The caller gets a
/// fresh handle to the same open terminal (the session keeps it alive), or `ENXIO` when it has no
/// controlling terminal for its session, exactly as FreeBSD's `cttyopen`/`ttyopen` path returns.
pub fn ctty_handle() -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let ids = *caller.ids.lock();
    let debug = debug_enabled();
    // A direct child keeps its own slot; a `posix_spawn` child starts without one and resolves
    // the session's controlling terminal (FreeBSD `cttyopen` uses the session's ctty).
    let ctty = match caller.ctty.lock().clone() {
        Some(ctty) => {
            if debug {
                kprintln!(
                    "ctty: PROC_CTTY_HANDLE pid={} sid={} source=own ctty_sid={}",
                    caller.id,
                    ids.sid,
                    ctty.sid()
                );
            }
            ctty
        }
        None => {
            let found = tty::session(ids.sid);
            if debug {
                kprintln!(
                    "ctty: PROC_CTTY_HANDLE pid={} sid={} source=session found={}",
                    caller.id,
                    ids.sid,
                    found.as_ref().map_or(0, |ctty| ctty.sid())
                );
            }
            found.ok_or(Errno::ENXIO)?
        }
    };
    if ctty.sid() == 0 || ctty.sid() != ids.sid {
        if debug {
            kprintln!(
                "ctty: PROC_CTTY_HANDLE pid={} sid={} reject ctty_sid={} ENXIO",
                caller.id,
                ids.sid,
                ctty.sid()
            );
        }
        return Err(Errno::ENXIO);
    }
    let file = Arc::clone(ctty.file());
    drop(ctty);
    let handle = caller
        .handles
        .lock()
        .insert(Object::SchemeFile(file), Rights::ALL)?;
    Ok(handle.0 as usize)
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
